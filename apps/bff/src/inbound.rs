//! Eventos que llegan del C5: mensajes del operador, estados de entrega,
//! cambios de estado del incidente y datos de la videollamada.
//!
//! Todas las réplicas consumen el flujo; cada evento se emite una sola vez
//! gracias a la llave de deduplicación `sb:c5e:<event_id>` dentro del script
//! `emit` (atómico). Así, si una réplica cae, las demás siguen entregando.

use std::{sync::Arc, time::Duration};

use events::{EventKind, RecordEvent, SensitivePayload};
use futures::StreamExt;
use protocol::{
    Author, ChatKind, IncidentState, IncidentStatus, MessageStatus, ServerChatMessage, ServerEvent,
    Timestamp, VideoCallUpdate,
};
use uuid::Uuid;

use crate::{
    AppState,
    backoff::Backoff,
    c5::{C5InboundEvent, C5InboundKind},
    forwarder::{ID_NAMESPACE, producer},
    store::EmitOutcome,
    valkey::{ValkeyError, c5_event_key},
};

/// Cuánto se recuerda un `event_id` del C5 para descartar repetidos.
const DEDUPE_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

pub async fn run(state: Arc<AppState>) {
    let mut stream = state.gateway.events();
    while let Some(event) = stream.next().await {
        let mut backoff = Backoff::new(Duration::from_millis(100), Duration::from_secs(5));
        // Si Valkey no responde, se reintenta: perder un mensaje del operador
        // no es opción. Mientras tanto el flujo hace contrapresión.
        while let Err(error) = handle(&state, &event).await {
            tracing::warn!(%error, event_id = %event.event_id, "no se pudo procesar evento del C5; reintentando");
            tokio::time::sleep(backoff.next_delay()).await;
        }
    }
    tracing::error!("el flujo de eventos del C5 terminó inesperadamente");
}

pub async fn handle(state: &Arc<AppState>, event: &C5InboundEvent) -> Result<(), ValkeyError> {
    let Some(session_id) = state
        .store
        .session_for_c5_incident(&event.c5_incident_id)
        .await?
    else {
        state.recorder.record(
            RecordEvent::new(EventKind::C5OrphanEvent, producer(state))
                .incident(None, Some(event.c5_incident_id.clone()))
                .attr("c5_event_id", event.event_id.clone()),
        );
        return Ok(());
    };

    let now = Timestamp::now();
    let mut fields: Vec<(&str, String)> = Vec::new();
    let (server_event, record) = match &event.kind {
        C5InboundKind::OperatorMessage {
            c5_message_id,
            text,
        } => (
            ServerEvent::ChatMessage(ServerChatMessage {
                message_id: Uuid::new_v5(&ID_NAMESPACE, event.event_id.as_bytes()),
                author: Author::Operator,
                kind: ChatKind::Text,
                text: text.clone(),
                quick_reply: None,
                client_msg_id: None,
                server_ts: now,
            }),
            RecordEvent::new(EventKind::OperatorMessage, producer(state))
                .attr("c5_message_id", c5_message_id.clone())
                .sensitive(SensitivePayload::Message { text: text.clone() }),
        ),
        C5InboundKind::MessageStatus {
            idempotency_key,
            status,
        } => (
            ServerEvent::MessageStatus(MessageStatus {
                client_msg_id: *idempotency_key,
                status: *status,
                server_ts: now,
            }),
            RecordEvent::new(EventKind::MessageStatusChanged, producer(state))
                .attr("client_msg_id", idempotency_key.to_string())
                .attr("status", serde_json::to_value(status).unwrap_or_default()),
        ),
        C5InboundKind::IncidentStatus {
            status,
            eta_seconds,
        } => {
            fields.push(("incident_status", status.as_str().to_owned()));
            if *status == IncidentState::Closed {
                // Tras el cierre, la sesión vuelve al TTL corto.
                fields.push((
                    "ttl_ms",
                    state.config.sessions.idle_ttl.as_millis().to_string(),
                ));
            }
            (
                ServerEvent::IncidentStatus(IncidentStatus {
                    status: *status,
                    eta_seconds: *eta_seconds,
                    server_ts: now,
                }),
                RecordEvent::new(EventKind::IncidentStatusChanged, producer(state))
                    .attr("status", status.as_str())
                    .attr("eta_seconds", *eta_seconds),
            )
        }
        C5InboundKind::VideoCall {
            idempotency_key,
            status,
            join,
        } => {
            let mut record = RecordEvent::new(EventKind::VideoCallUpdated, producer(state))
                .attr("client_msg_id", idempotency_key.to_string())
                .attr("status", serde_json::to_value(status).unwrap_or_default());
            if let Some(data) = join {
                record = record.sensitive(SensitivePayload::VideoCall { data: data.clone() });
            }
            (
                ServerEvent::VideoCall(VideoCallUpdate {
                    request_client_msg_id: *idempotency_key,
                    status: *status,
                    join: join.clone(),
                    server_ts: now,
                }),
                record,
            )
        }
    };

    let dedupe_key = c5_event_key(&event.event_id);
    let outcome = state
        .store
        .emit(
            session_id,
            &server_event,
            Some((&dedupe_key, DEDUPE_TTL)),
            &fields,
        )
        .await?;
    let seq = match outcome {
        EmitOutcome::Emitted(seq) => seq,
        // Repetido (otra réplica ya lo emitió) o sesión expirada.
        EmitOutcome::Duplicate => return Ok(()),
        EmitOutcome::SessionGone => {
            state.recorder.record(
                RecordEvent::new(EventKind::C5OrphanEvent, producer(state))
                    .session(session_id)
                    .incident(None, Some(event.c5_incident_id.clone()))
                    .attr("c5_event_id", event.event_id.clone())
                    .attr("reason", "session_expired"),
            );
            return Ok(());
        }
    };

    let session = state
        .store
        .session_fields(session_id)
        .await
        .unwrap_or_default();
    let incident_id = session.get("incident_id").and_then(|v| v.parse().ok());
    state.recorder.record(
        record
            .session(session_id)
            .incident(incident_id, Some(event.c5_incident_id.clone()))
            .attr("c5_event_id", event.event_id.clone())
            .attr("c5_occurred_at", event.occurred_at.to_string())
            .attr("seq", seq),
    );

    // Primera respuesta del operador: métrica clave del programa.
    if matches!(event.kind, C5InboundKind::OperatorMessage { .. })
        && state
            .store
            .set_field_once(
                session_id,
                "first_operator_at",
                &now.unix_millis().to_string(),
            )
            .await?
    {
        let latency_ms = session
            .get("incident_created_at")
            .and_then(|v| v.parse::<i64>().ok())
            .map(|created| now.unix_millis() - created);
        state.recorder.record(
            RecordEvent::new(EventKind::FirstOperatorResponse, producer(state))
                .session(session_id)
                .incident(incident_id, Some(event.c5_incident_id.clone()))
                .attr("latency_ms", latency_ms),
        );
    }
    Ok(())
}
