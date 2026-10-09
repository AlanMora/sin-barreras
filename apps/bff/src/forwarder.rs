//! Reenvío durable y ordenado de los comandos del ciudadano al C5.
//!
//! Garantías (ADR-0011):
//! - **No pérdida**: un comando se confirma al ciudadano solo cuando ya está
//!   en el stream `sb:s:{sid}:in` de Valkey (ver `accept.lua`). El forwarder
//!   lo saca de ahí; si la réplica muere a medias, otra lo retoma.
//! - **Orden**: por sesión hay un solo forwarder a la vez (lease en Valkey) y
//!   procesa los comandos en el orden en que se aceptaron.
//! - **Sin duplicados en el C5**: cada llamada lleva como llave de
//!   idempotencia el `client_msg_id`.
//! - **Persistencia ante fallas del C5**: las fallas temporales se reintentan
//!   indefinidamente con backoff mientras la réplica conserve el lease.
//! - **Registro atómico**: lo que ve el ciudadano (incidente creado, entregado,
//!   fallido…) se registra en la misma operación que se le emite.

use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
    time::Duration,
};

use events::{EventKind, RecordEvent, SensitivePayload};
use protocol::{
    Channel, ChatKind, ClientCloseReason, ClientMessage, DeliveryStatus, IncidentCreated,
    IncidentState, IncidentStatus, LocationSource, MessageStatus, ServerEvent, SessionClosed,
    SessionClosedReason, Timestamp, VideoCallState, VideoCallUpdate,
};
use service_kit::backoff::Backoff;
use uuid::Uuid;

use crate::{
    AppState,
    c5::{C5Error, LocationReport, NewIncident, OutboundMessage, VideoCallRequest},
    records::SEQ_PLACEHOLDER,
    store::{PendingCommand, ReleaseOutcome},
    valkey::ValkeyError,
};

/// Antigüedad mínima de un pendiente para que el barrido lo retome.
pub const SWEEP_AFTER: Duration = Duration::from_secs(3);

/// Espacio de nombres para derivar ids estables (UUIDv5) a partir de ids del cliente o del C5.
pub const ID_NAMESPACE: Uuid = Uuid::from_u128(0x5b1c_0b5e_7a2d_4c3e_9f10_c5c5_0000_0001);

/// Forwarders activos en esta réplica. `true` = hay que volver a revisar al terminar.
#[derive(Default)]
pub struct Forwarders {
    running: Mutex<HashMap<Uuid, bool>>,
}

impl Forwarders {
    pub fn active(&self) -> usize {
        self.running.lock().map(|r| r.len()).unwrap_or(0)
    }
}

/// Pide que se reenvíen los pendientes de una sesión. Nunca bloquea.
pub fn kick(state: &Arc<AppState>, session_id: Uuid) {
    let start = match state.forwarders.running.lock() {
        Ok(mut running) => match running.get_mut(&session_id) {
            Some(rerun) => {
                *rerun = true;
                false
            }
            None => {
                running.insert(session_id, false);
                true
            }
        },
        Err(_) => false,
    };
    if start {
        let state = Arc::clone(state);
        tokio::spawn(async move {
            loop {
                if let Err(error) = forward_session(&state, session_id).await {
                    tracing::warn!(%session_id, %error, "forwarder interrumpido; el barrido lo retomará");
                }
                let again = match state.forwarders.running.lock() {
                    Ok(mut running) => {
                        if running.get(&session_id).copied().unwrap_or(false) {
                            running.insert(session_id, false);
                            true
                        } else {
                            running.remove(&session_id);
                            false
                        }
                    }
                    Err(_) => false,
                };
                if !again {
                    break;
                }
            }
        });
    }
}

/// Barrido periódico: retoma sesiones con pendientes viejos (p. ej. de una
/// réplica que murió). Corre en todas las réplicas; el lease evita choques.
pub async fn run_sweeper(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        let threshold = Timestamp::now().unix_millis()
            - i64::try_from(SWEEP_AFTER.as_millis()).unwrap_or(3_000);
        match state.store.sweep_candidates(threshold, 100).await {
            Ok(sessions) => {
                for session_id in sessions {
                    kick(&state, session_id);
                }
            }
            Err(error) => tracing::warn!(%error, "barrido de pendientes falló"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum ForwardError {
    #[error(transparent)]
    Valkey(#[from] ValkeyError),
    #[error("se perdió el lease de reenvío")]
    LostLease,
    #[error("la sesión ya no existe")]
    SessionGone,
}

/// Contexto de la sesión que el forwarder necesita.
#[derive(Debug, Default, Clone)]
struct SessionContext {
    fields: HashMap<String, String>,
}

impl SessionContext {
    fn get(&self, field: &str) -> Option<&str> {
        self.fields
            .get(field)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }
    fn incident_id(&self) -> Option<Uuid> {
        self.get("incident_id").and_then(|v| v.parse().ok())
    }
    fn c5_incident_id(&self) -> Option<String> {
        self.get("c5_incident_id").map(str::to_owned)
    }
    fn channel(&self) -> Option<Channel> {
        self.get("channel")
            .and_then(|c| serde_json::from_value(serde_json::Value::String(c.to_owned())).ok())
    }
}

async fn forward_session(state: &Arc<AppState>, session_id: Uuid) -> Result<(), ForwardError> {
    let owner = state.config.replica_id.as_str();
    loop {
        if !state
            .store
            .lease_acquire(session_id, owner, state.config.forward_lease_ttl)
            .await?
        {
            // Otra réplica está reenviando esta sesión.
            return Ok(());
        }
        let fields = state.store.session_fields(session_id).await?;
        if fields.is_empty() {
            // La sesión expiró: liberar (también la saca del barrido).
            state
                .store
                .lease_release_if_drained(session_id, owner)
                .await?;
            return Err(ForwardError::SessionGone);
        }
        let mut ctx = SessionContext { fields };
        let after = ctx.get("fwd_id").unwrap_or("0-0").to_owned();
        let pending = state.store.pending_commands(session_id, &after, 20).await?;
        if pending.is_empty() {
            match state
                .store
                .lease_release_if_drained(session_id, owner)
                .await?
            {
                ReleaseOutcome::Pending => continue,
                ReleaseOutcome::Released | ReleaseOutcome::NotOwner => return Ok(()),
            }
        }
        for command in pending {
            process(state, session_id, &mut ctx, &command).await?;
        }
    }
}

async fn process(
    state: &Arc<AppState>,
    session_id: Uuid,
    ctx: &mut SessionContext,
    command: &PendingCommand,
) -> Result<(), ForwardError> {
    let message: ClientMessage = match serde_json::from_str(&command.message_json) {
        Ok(m) => m,
        Err(error) => {
            // No debería pasar: el BFF guardó este JSON. Se registra y se salta.
            tracing::error!(%session_id, %error, "comando ilegible en el stream de entrada");
            return mark(state, session_id, &command.stream_id, &[]).await;
        }
    };
    let cmid = message.client_msg_id();
    let mut fields: Vec<(&str, String)> = Vec::new();

    match &message {
        ClientMessage::CreateIncident(m) => {
            let language = m
                .language
                .clone()
                .or_else(|| ctx.get("language").map(str::to_owned));
            ensure_incident(state, session_id, ctx, m.channel, cmid, language).await?;
        }
        ClientMessage::ChatMessage(m) => {
            let channel = ctx.channel().unwrap_or(match m.kind {
                ChatKind::QuickReply => Channel::Silent,
                ChatKind::Text => Channel::TextChat,
            });
            let Some(c5_incident_id) =
                ensure_incident(state, session_id, ctx, channel, cmid, None).await?
            else {
                let record = base(state, EventKind::CitizenMessageFailed, session_id, ctx)
                    .attr("client_msg_id", cmid.to_string())
                    .attr("reason", "incident_not_created");
                emit_status(state, session_id, cmid, DeliveryStatus::Failed, record).await?;
                return mark(state, session_id, &command.stream_id, &[]).await;
            };
            let outbound = OutboundMessage {
                idempotency_key: cmid,
                c5_incident_id,
                kind: m.kind,
                text: m.text.clone(),
                quick_reply: m.quick_reply,
                client_ts: m.client_ts,
            };
            match with_retry(state, session_id, "send_message", || {
                state.gateway.send_message(outbound.clone())
            })
            .await?
            {
                Ok(c5_message_id) => {
                    let record = base(state, EventKind::CitizenMessageForwarded, session_id, ctx)
                        .attr("client_msg_id", cmid.to_string())
                        .attr("c5_message_id", c5_message_id);
                    emit_status(state, session_id, cmid, DeliveryStatus::Delivered, record).await?;
                }
                Err(reason) => {
                    let record = base(state, EventKind::CitizenMessageFailed, session_id, ctx)
                        .attr("client_msg_id", cmid.to_string())
                        .attr("reason", reason);
                    emit_status(state, session_id, cmid, DeliveryStatus::Failed, record).await?;
                }
            }
        }
        ClientMessage::LocationUpdate(m) => match ctx.c5_incident_id() {
            Some(c5_incident_id) => {
                let report = location_report(m, c5_incident_id);
                forward_location(state, session_id, ctx, report).await?;
            }
            // Aún no hay incidente: se guarda la más reciente y se envía al crearlo.
            None => fields.push(("pending_location", command.message_json.clone())),
        },
        ClientMessage::VideoCallRequest(_) => {
            let channel = ctx.channel().unwrap_or(Channel::LsmVideo);
            let Some(c5_incident_id) =
                ensure_incident(state, session_id, ctx, channel, cmid, None).await?
            else {
                let record = base(state, EventKind::VideoCallUpdated, session_id, ctx)
                    .attr("client_msg_id", cmid.to_string())
                    .attr("status", "failed")
                    .attr("reason", "incident_not_created");
                emit_video(state, session_id, cmid, VideoCallState::Failed, record).await?;
                return mark(state, session_id, &command.stream_id, &[]).await;
            };
            let request = VideoCallRequest {
                idempotency_key: cmid,
                c5_incident_id,
            };
            match with_retry(state, session_id, "request_video_call", || {
                state.gateway.request_video_call(request.clone())
            })
            .await?
            {
                Ok(video_call_id) => {
                    let record = base(state, EventKind::VideoCallRequested, session_id, ctx)
                        .attr("client_msg_id", cmid.to_string())
                        .attr("c5_video_call_id", video_call_id)
                        .attr("stage", "forwarded");
                    emit_video(state, session_id, cmid, VideoCallState::Requested, record).await?;
                }
                Err(reason) => {
                    let record = base(state, EventKind::VideoCallUpdated, session_id, ctx)
                        .attr("client_msg_id", cmid.to_string())
                        .attr("status", "failed")
                        .attr("reason", reason);
                    emit_video(state, session_id, cmid, VideoCallState::Failed, record).await?;
                }
            }
        }
        ClientMessage::Close(m) => {
            let reason = match m.reason {
                ClientCloseReason::EndedByCitizen | ClientCloseReason::AbandonedBeforeHelp => {
                    SessionClosedReason::EndedByCitizen
                }
            };
            let record = base(state, EventKind::SessionClosed, session_id, ctx)
                .attr("reason", serde_json::to_value(m.reason).unwrap_or_default())
                .attr("by", "citizen");
            let event = ServerEvent::SessionClosed(SessionClosed {
                reason,
                server_ts: Timestamp::now(),
            });
            let idle_ttl = state.config.sessions.idle_ttl.as_millis().to_string();
            emit_recorded(
                state,
                session_id,
                &event,
                &[("closed", "1".into()), ("ttl_ms", idle_ttl)],
                record,
            )
            .await?;
        }
        // Estos tipos nunca entran al stream de reenvío.
        ClientMessage::Hello(_)
        | ClientMessage::Ack(_)
        | ClientMessage::Ping(_)
        | ClientMessage::ClientEvent(_) => {}
    }

    mark(state, session_id, &command.stream_id, &fields).await
}

async fn mark(
    state: &Arc<AppState>,
    session_id: Uuid,
    stream_id: &str,
    fields: &[(&str, String)],
) -> Result<(), ForwardError> {
    if state
        .store
        .mark_forwarded(session_id, &state.config.replica_id, stream_id, fields)
        .await?
    {
        Ok(())
    } else {
        Err(ForwardError::LostLease)
    }
}

/// Garantiza que exista el incidente en el C5. Devuelve su id, o `None` si el
/// C5 lo rechazó de forma definitiva.
async fn ensure_incident(
    state: &Arc<AppState>,
    session_id: Uuid,
    ctx: &mut SessionContext,
    channel: Channel,
    cause: Uuid,
    language: Option<String>,
) -> Result<Option<String>, ForwardError> {
    if let Some(id) = ctx.c5_incident_id() {
        return Ok(Some(id));
    }
    // Id estable derivado del mensaje que lo provocó: si otra réplica reintenta,
    // obtiene el mismo id.
    let incident_id = Uuid::new_v5(&ID_NAMESPACE, cause.as_bytes());
    let entry_status = ctx.get("entry_status").unwrap_or("none").to_owned();
    let request = NewIncident {
        idempotency_key: cause,
        incident_id,
        channel,
        language: language
            .or_else(|| ctx.get("language").map(str::to_owned))
            .unwrap_or_else(|| "es-MX".into()),
        entry_point: ctx.get("entry_point").map(str::to_owned),
        entry_verified: entry_status == "verified",
    };
    let c5_incident_id = match with_retry(state, session_id, "create_incident", || {
        state.gateway.create_incident(request.clone())
    })
    .await?
    {
        Ok(id) => id,
        Err(reason) => {
            state.records.record(
                base(state, EventKind::C5CallFailed, session_id, ctx)
                    .attr("operation", "create_incident")
                    .attr("final", true)
                    .attr("reason", reason),
            );
            return Ok(None);
        }
    };

    let now = Timestamp::now();
    let incident_ttl = state.config.sessions.incident_ttl;
    state
        .store
        .map_c5_incident(&c5_incident_id, session_id, incident_ttl)
        .await?;
    let new_fields: Vec<(&str, String)> = vec![
        ("incident_id", incident_id.to_string()),
        ("c5_incident_id", c5_incident_id.clone()),
        ("channel", channel.as_str().to_owned()),
        (
            "incident_status",
            IncidentState::Received.as_str().to_owned(),
        ),
        ("incident_created_at", now.unix_millis().to_string()),
        ("ttl_ms", incident_ttl.as_millis().to_string()),
    ];
    for (field, value) in &new_fields {
        ctx.fields.insert((*field).to_owned(), value.clone());
    }
    // Identificadores cruzados (nuestro incidente ↔ el del C5) en el mismo
    // registro atómico que avisa al ciudadano.
    let record = base(state, EventKind::IncidentCreated, session_id, ctx)
        .attr("channel", channel.as_str())
        .attr("cause_client_msg_id", cause.to_string())
        .attr("entry_point", ctx.get("entry_point").unwrap_or_default())
        .attr("entry_status", entry_status);
    let created = ServerEvent::IncidentCreated(IncidentCreated {
        incident_id,
        channel,
        client_msg_id: cause,
        server_ts: now,
    });
    emit_recorded(state, session_id, &created, &new_fields, record).await?;
    let record =
        base(state, EventKind::IncidentStatusChanged, session_id, ctx).attr("status", "received");
    let status = ServerEvent::IncidentStatus(IncidentStatus {
        status: IncidentState::Received,
        eta_seconds: None,
        server_ts: now,
    });
    emit_recorded(state, session_id, &status, &[], record).await?;

    // Ubicación GPS que llegó antes del incidente.
    if let Some(pending) = ctx.get("pending_location").map(str::to_owned)
        && let Ok(ClientMessage::LocationUpdate(m)) =
            serde_json::from_str::<ClientMessage>(&pending)
    {
        let report = location_report(&m, c5_incident_id.clone());
        forward_location(state, session_id, ctx, report).await?;
        state
            .store
            .set_fields(session_id, &[("pending_location", String::new())])
            .await?;
        ctx.fields.remove("pending_location");
    }
    Ok(Some(c5_incident_id))
}

fn location_report(m: &protocol::LocationUpdate, c5_incident_id: String) -> LocationReport {
    LocationReport {
        idempotency_key: m.client_msg_id,
        c5_incident_id,
        latitude: m.latitude,
        longitude: m.longitude,
        accuracy_m: m.accuracy_m,
        source: source_name(m.source).to_owned(),
        captured_at: m.captured_at,
    }
}

async fn forward_location(
    state: &Arc<AppState>,
    session_id: Uuid,
    ctx: &SessionContext,
    report: LocationReport,
) -> Result<(), ForwardError> {
    let key = report.idempotency_key;
    let source = report.source.clone();
    match with_retry(state, session_id, "update_location", || {
        state.gateway.update_location(report.clone())
    })
    .await?
    {
        Ok(()) => state.records.record(
            base(state, EventKind::LocationForwarded, session_id, ctx)
                .attr("client_msg_id", key.to_string())
                .attr("source", source),
        ),
        Err(reason) => state.records.record(
            base(state, EventKind::C5CallFailed, session_id, ctx)
                .attr("operation", "update_location")
                .attr("final", true)
                .attr("reason", reason),
        ),
    }
    Ok(())
}

/// Ejecuta una llamada al C5 con reintentos. Devuelve `Ok(Err(motivo))` si el
/// C5 la rechazó de forma definitiva.
async fn with_retry<T, F, Fut>(
    state: &Arc<AppState>,
    session_id: Uuid,
    operation: &'static str,
    mut call: F,
) -> Result<Result<T, String>, ForwardError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, C5Error>>,
{
    let owner = state.config.replica_id.as_str();
    let mut backoff = Backoff::new(Duration::from_millis(200), Duration::from_secs(10));
    loop {
        if !state
            .store
            .lease_acquire(session_id, owner, state.config.forward_lease_ttl)
            .await?
        {
            return Err(ForwardError::LostLease);
        }
        match call().await {
            Ok(value) => return Ok(Ok(value)),
            Err(C5Error::Rejected(reason)) => return Ok(Err(reason)),
            Err(C5Error::Transient(reason)) => {
                let attempt = backoff.attempt() + 1;
                tracing::warn!(%session_id, operation, attempt, "falla temporal del C5; reintentando");
                // Se registra el 1.º, 2.º, 4.º, 8.º… intento para no inundar el registro.
                if attempt.is_power_of_two() {
                    state.records.record(
                        RecordEvent::new(EventKind::C5CallFailed, producer(state))
                            .session(session_id)
                            .attr("operation", operation)
                            .attr("attempt", attempt)
                            .attr("final", false)
                            .attr("reason", reason),
                    );
                }
                tokio::time::sleep(backoff.next_delay()).await;
            }
        }
    }
}

/// Emite al ciudadano y registra, en una sola operación atómica.
async fn emit_recorded(
    state: &Arc<AppState>,
    session_id: Uuid,
    event: &ServerEvent,
    fields: &[(&str, String)],
    record: RecordEvent,
) -> Result<(), ForwardError> {
    let records = state
        .records
        .encode_all(vec![record.attr("seq", SEQ_PLACEHOLDER)])
        .await;
    state
        .store
        .emit(session_id, event, None, fields, &records)
        .await?;
    Ok(())
}

async fn emit_status(
    state: &Arc<AppState>,
    session_id: Uuid,
    client_msg_id: Uuid,
    status: DeliveryStatus,
    record: RecordEvent,
) -> Result<(), ForwardError> {
    let event = ServerEvent::MessageStatus(MessageStatus {
        client_msg_id,
        status,
        server_ts: Timestamp::now(),
    });
    emit_recorded(state, session_id, &event, &[], record).await
}

async fn emit_video(
    state: &Arc<AppState>,
    session_id: Uuid,
    client_msg_id: Uuid,
    status: VideoCallState,
    record: RecordEvent,
) -> Result<(), ForwardError> {
    let event = ServerEvent::VideoCall(VideoCallUpdate {
        request_client_msg_id: client_msg_id,
        status,
        join: None,
        server_ts: Timestamp::now(),
    });
    emit_recorded(state, session_id, &event, &[], record).await
}

fn source_name(source: LocationSource) -> &'static str {
    match source {
        LocationSource::Gps => "gps",
        LocationSource::Network => "network",
    }
}

pub fn producer(state: &AppState) -> String {
    state.records.producer().to_owned()
}

fn base(state: &AppState, kind: EventKind, session_id: Uuid, ctx: &SessionContext) -> RecordEvent {
    RecordEvent::new(kind, producer(state))
        .session(session_id)
        .incident(ctx.incident_id(), ctx.c5_incident_id())
}

/// Contenido sensible de una ubicación (para `ws.rs`).
pub fn location_payload(m: &protocol::LocationUpdate) -> SensitivePayload {
    SensitivePayload::Location {
        latitude: m.latitude,
        longitude: m.longitude,
        accuracy_m: m.accuracy_m,
        source: source_name(m.source).to_owned(),
        captured_at: m.captured_at,
    }
}
