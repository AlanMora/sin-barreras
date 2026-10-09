//! Lógica de sesión y de comandos compartida por los dos transportes
//! (WebSocket y HTTP de respaldo). Ningún transporte acepta, valida o
//! registra un comando por su cuenta: todo pasa por aquí.

use std::{net::SocketAddr, sync::Arc};

use axum::http::{HeaderMap, header};
use events::{EventKind, RecordEvent, SensitivePayload};
use protocol::{
    AckStatus, Author, ClientMessage, EntryVerification, ErrorCode, HEARTBEAT_INTERVAL_MS, Hello,
    IncidentState, IncidentSummary, MAX_CLIENT_FRAME_BYTES, PROTOCOL_VERSION, Pong, ServerAck,
    ServerChatMessage, ServerControl, ServerError, ServerEvent, Timestamp, Welcome,
};
use uuid::Uuid;

use crate::{
    AppState,
    forwarder::{self, ID_NAMESPACE, location_payload, producer},
    records::SEQ_PLACEHOLDER,
    store::{AcceptCommand, AcceptOutcome, ResumeOutcome},
    token,
    valkey::ValkeyError,
};

/// Datos de red del cliente. Sensibles: solo se guardan cifrados en el registro.
#[derive(Debug, Clone)]
pub struct NetworkInfo {
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

impl NetworkInfo {
    pub fn from_request(headers: &HeaderMap, peer: SocketAddr) -> Self {
        Self {
            ip: Some(client_ip(headers, peer)),
            user_agent: headers
                .get(header::USER_AGENT)
                .and_then(|v| v.to_str().ok())
                .map(|ua| ua.chars().take(512).collect()),
        }
    }
}

/// Los navegadores siempre envían `Origin` en un WebSocket y en un POST entre
/// orígenes; se valida para evitar ataques entre sitios (CSWSH/CSRF). Los
/// clientes que no son navegador (pruebas, herramientas) no lo envían.
pub fn origin_allowed(allowed: &[String], headers: &HeaderMap) -> bool {
    match headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        None => true,
        Some(origin) => allowed
            .iter()
            .any(|a| a == "*" || a.eq_ignore_ascii_case(origin)),
    }
}

/// IP del cliente: el BFF solo es alcanzable a través de Traefik, que agrega
/// la IP real al final de `X-Forwarded-For`.
pub fn client_ip(headers: &HeaderMap, peer: SocketAddr) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit(',').next())
        .map(str::trim)
        .filter(|ip| ip.parse::<std::net::IpAddr>().is_ok())
        .map(str::to_owned)
        .unwrap_or_else(|| peer.ip().to_string())
}

pub fn error(
    code: ErrorCode,
    message: String,
    client_msg_id: Option<Uuid>,
    retryable: bool,
) -> ServerControl {
    ServerControl::Error(ServerError {
        code,
        message,
        client_msg_id,
        retryable,
        supported_versions: None,
        server_ts: Timestamp::now(),
    })
}

pub fn unsupported_version(hello: &Hello) -> ServerControl {
    let mut err = error(
        ErrorCode::UnsupportedProtocolVersion,
        format!("versión {} no soportada", hello.protocol_version),
        Some(hello.client_msg_id),
        false,
    );
    if let ServerControl::Error(e) = &mut err {
        e.supported_versions = Some(vec![PROTOCOL_VERSION]);
    }
    err
}

/// Intenta recuperar el `client_msg_id` de un JSON inválido para que el
/// cliente sepa a qué mensaje corresponde el error.
pub fn extract_client_msg_id(text: &str) -> Option<Uuid> {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()?
        .get("client_msg_id")?
        .as_str()?
        .parse()
        .ok()
}

pub type HelloError = (ErrorCode, String, Option<Uuid>);

pub fn parse_hello(text: &str) -> Result<(Hello, Vec<&'static str>), HelloError> {
    let mut message: ClientMessage = serde_json::from_str(text).map_err(|_| {
        (
            ErrorCode::InvalidMessage,
            "hello no válido".to_owned(),
            extract_client_msg_id(text),
        )
    })?;
    let dropped = message.sanitize();
    message.validate().map_err(|e| {
        (
            ErrorCode::InvalidMessage,
            e.to_string(),
            Some(message.client_msg_id()),
        )
    })?;
    match message {
        ClientMessage::Hello(hello) => Ok((hello, dropped)),
        other => Err((
            ErrorCode::HelloRequired,
            "el primer mensaje debe ser hello".into(),
            Some(other.client_msg_id()),
        )),
    }
}

pub struct Established {
    pub session_id: Uuid,
    pub token: String,
    pub resumed: bool,
    pub entry: EntryVerification,
    /// `seq` desde el que hay que reenviar (0 en una sesión nueva).
    pub replay_after: u64,
}

/// Crea o reanuda la sesión de un `hello`. Los registros iniciales se
/// escriben en la misma operación atómica que la sesión.
pub async fn establish_session(
    state: &Arc<AppState>,
    hello: &Hello,
    dropped: &[&'static str],
    network: &NetworkInfo,
) -> Result<Established, Box<dyn std::error::Error + Send + Sync>> {
    let token = token::generate()?;
    let token_hash = token::hash(&token);
    let now = Timestamp::now();

    let established = 'resume: {
        let Some(resume) = &hello.resume else {
            break 'resume None;
        };
        let records = state
            .records
            .encode_all(vec![
                RecordEvent::new(EventKind::SessionResumed, producer(state))
                    .session(resume.session_id)
                    .client_ts(hello.client_ts)
                    .attr("client_last_seq", resume.last_seq),
                metadata_event(state, resume.session_id, hello, network),
            ])
            .await;
        let outcome = state
            .store
            .resume_session(
                resume.session_id,
                &token::hash(&resume.session_token),
                &token_hash,
                &records,
            )
            .await?;
        if let ResumeOutcome::Resumed { .. } = outcome {
            let entry = match state
                .store
                .session_fields(resume.session_id)
                .await?
                .get("entry_status")
                .map(String::as_str)
            {
                Some("verified") => EntryVerification::Verified,
                Some("unverified") => EntryVerification::Unverified,
                _ => EntryVerification::None,
            };
            break 'resume Some(Established {
                session_id: resume.session_id,
                token: token.clone(),
                resumed: true,
                entry,
                replay_after: resume.last_seq,
            });
        }
        state.records.record(
            RecordEvent::new(EventKind::SessionResumeRejected, producer(state))
                .attr("requested_session_id", resume.session_id.to_string())
                .attr("reason", format!("{outcome:?}").to_lowercase()),
        );
        None
    };

    let established = match established {
        Some(e) => e,
        None => {
            let session_id = Uuid::now_v7();
            let entry = state.qr.verify(hello.entry.as_ref());
            let point_id = hello
                .entry
                .as_ref()
                .map(|e| e.point_id.clone())
                .unwrap_or_default();
            let key = state.records.new_session_key(session_id)?;
            let fields = vec![
                ("entry_point", point_id.clone()),
                ("entry_status", entry.as_str().to_owned()),
                ("language", hello.language.clone()),
                ("app_version", hello.app_version.clone()),
                ("protocol_version", hello.protocol_version.to_string()),
                ("dek", key.operational_wrapped),
            ];
            // La llave de la sesión va primero: todo lo demás se cifra con ella.
            let records = state
                .records
                .encode_all(vec![
                    key.event,
                    RecordEvent::new(EventKind::SessionStarted, producer(state))
                        .session(session_id)
                        .client_ts(hello.client_ts),
                    RecordEvent::new(EventKind::EntryPoint, producer(state))
                        .session(session_id)
                        .attr("source", if hello.entry.is_some() { "qr" } else { "web" })
                        .attr("point_id", point_id)
                        .attr(
                            "key_id",
                            hello.entry.as_ref().and_then(|e| e.key_id.clone()),
                        )
                        .attr("verification", entry.as_str()),
                    metadata_event(state, session_id, hello, network),
                ])
                .await;
            state
                .store
                .create_session(
                    session_id,
                    &token_hash,
                    now.unix_millis(),
                    state.config.sessions.idle_ttl,
                    &fields,
                    &records,
                )
                .await?;
            Established {
                session_id,
                token,
                resumed: false,
                entry,
                replay_after: 0,
            }
        }
    };

    if !dropped.is_empty() {
        state.records.record(
            RecordEvent::new(EventKind::FieldsDropped, producer(state))
                .session(established.session_id)
                .attr("message_type", "hello")
                .attr("fields", dropped.to_vec()),
        );
    }
    Ok(established)
}

/// Metadatos técnicos del cliente. La IP y el user agent van cifrados.
fn metadata_event(
    state: &Arc<AppState>,
    session_id: Uuid,
    hello: &Hello,
    network: &NetworkInfo,
) -> RecordEvent {
    RecordEvent::new(EventKind::ClientMetadata, producer(state))
        .session(session_id)
        .client_ts(hello.client_ts)
        .trace(hello.traceparent.as_deref())
        .attr("language", hello.language.clone())
        .attr("app_version", hello.app_version.clone())
        .attr("protocol_version", hello.protocol_version)
        .attr(
            "accessibility",
            serde_json::to_value(&hello.accessibility).unwrap_or_default(),
        )
        .sensitive(SensitivePayload::Network {
            ip: network.ip.clone(),
            user_agent: network.user_agent.clone(),
        })
}

pub async fn welcome(state: &AppState, est: &Established) -> Result<Welcome, ValkeyError> {
    let fields = state.store.session_fields(est.session_id).await?;
    let latest_seq = state.store.latest_seq(est.session_id).await?;
    let incident = match (
        fields.get("incident_id").and_then(|v| v.parse().ok()),
        parse_field(&fields, "channel"),
    ) {
        (Some(incident_id), Some(channel)) => Some(IncidentSummary {
            incident_id,
            channel,
            status: parse_field(&fields, "incident_status").unwrap_or(IncidentState::Received),
        }),
        _ => None,
    };
    let ttl_ms = fields
        .get("ttl_ms")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);
    let now = Timestamp::now();
    Ok(Welcome {
        protocol_version: PROTOCOL_VERSION,
        session_id: est.session_id,
        session_token: est.token.clone(),
        token_expires_at: Timestamp::from_unix_millis(now.unix_millis() + ttl_ms).unwrap_or(now),
        resumed: est.resumed,
        entry: est.entry,
        heartbeat_interval_ms: HEARTBEAT_INTERVAL_MS,
        latest_seq,
        incident,
        server_ts: now,
    })
}

/// Lee un enum serde guardado como texto en la sesión (`text_chat`, `attending`…).
fn parse_field<T: serde::de::DeserializeOwned>(
    fields: &std::collections::HashMap<String, String>,
    name: &str,
) -> Option<T> {
    fields
        .get(name)
        .and_then(|v| serde_json::from_value(serde_json::Value::String(v.clone())).ok())
}

/// Qué hacer tras procesar un mensaje del cliente.
#[derive(Debug)]
pub enum CommandReply {
    /// Responder este mensaje de control.
    Reply(ServerControl),
    /// Nada que responder (p. ej. un `ack` del cliente).
    Nothing,
    /// La sesión expiró: responder y obligar a un `hello` nuevo.
    SessionGone(ServerControl),
}

/// Procesa un mensaje del cliente (cualquier tipo salvo `hello` inicial).
pub async fn handle_command(state: &Arc<AppState>, session_id: Uuid, text: &str) -> CommandReply {
    let record_protocol_error = |stage: &str, reason: String, client_msg_id: Option<Uuid>| {
        state.records.record(
            RecordEvent::new(EventKind::ProtocolError, producer(state))
                .session(session_id)
                .attr("stage", stage)
                .attr("reason", reason)
                .attr("client_msg_id", client_msg_id.map(|id| id.to_string())),
        );
    };
    if text.len() > MAX_CLIENT_FRAME_BYTES {
        return CommandReply::Reply(error(
            ErrorCode::MessageTooLarge,
            "mensaje demasiado grande".into(),
            None,
            false,
        ));
    }
    let mut message: ClientMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            let client_msg_id = extract_client_msg_id(text);
            record_protocol_error("parse", format!("{:?}", e.classify()), client_msg_id);
            return CommandReply::Reply(error(
                ErrorCode::InvalidMessage,
                "mensaje no válido".into(),
                client_msg_id,
                false,
            ));
        }
    };
    let dropped = message.sanitize();
    if !dropped.is_empty() {
        state.records.record(
            RecordEvent::new(EventKind::FieldsDropped, producer(state))
                .session(session_id)
                .attr("message_type", message.kind())
                .attr("client_msg_id", message.client_msg_id().to_string())
                .attr("fields", dropped),
        );
    }
    let cmid = message.client_msg_id();
    if let Err(invalid) = message.validate() {
        record_protocol_error("validation", invalid.to_string(), Some(cmid));
        return CommandReply::Reply(error(
            ErrorCode::InvalidMessage,
            invalid.to_string(),
            Some(cmid),
            false,
        ));
    }

    match &message {
        ClientMessage::Hello(_) => CommandReply::Reply(error(
            ErrorCode::InvalidMessage,
            "hello duplicado".into(),
            Some(cmid),
            false,
        )),
        ClientMessage::Ping(_) => {
            let _ = state.store.touch(session_id).await;
            CommandReply::Reply(ServerControl::Pong(Pong {
                client_msg_id: cmid,
                server_ts: Timestamp::now(),
            }))
        }
        ClientMessage::Ack(ack) => {
            let _ = state
                .store
                .set_fields(session_id, &[("last_ack_seq", ack.seq.to_string())])
                .await;
            state.records.record(
                RecordEvent::new(EventKind::ClientAck, producer(state))
                    .session(session_id)
                    .client_ts(ack.client_ts)
                    .attr("seq", ack.seq),
            );
            CommandReply::Nothing
        }
        _ => accept(state, session_id, message).await,
    }
}

/// Acepta un comando de forma durable (con su registro, en la misma operación
/// atómica) y devuelve el `ack`. Si no se puede guardar, NO hay ack: el
/// cliente lo conserva en su cola y lo reintenta.
async fn accept(state: &Arc<AppState>, session_id: Uuid, message: ClientMessage) -> CommandReply {
    let cmid = message.client_msg_id();
    let now = Timestamp::now();
    let forward = !matches!(message, ClientMessage::ClientEvent(_));
    let echo = match &message {
        ClientMessage::ChatMessage(m) => Some(ServerEvent::ChatMessage(ServerChatMessage {
            message_id: Uuid::new_v5(&ID_NAMESPACE, cmid.as_bytes()),
            author: Author::Citizen,
            kind: m.kind,
            text: m.text.clone(),
            quick_reply: m.quick_reply,
            client_msg_id: Some(cmid),
            server_ts: now,
        })),
        _ => None,
    };
    let Ok(json) = serde_json::to_string(&message) else {
        return CommandReply::Reply(error(
            ErrorCode::Internal,
            "error interno".into(),
            Some(cmid),
            true,
        ));
    };
    let records = match accepted_record(state, session_id, &message) {
        Some(event) => state.records.encode_all(vec![event]).await,
        None => Vec::new(),
    };
    let command = AcceptCommand {
        client_msg_id: cmid,
        message_json: &json,
        now_ms: now.unix_millis(),
        forward,
        echo: echo.as_ref(),
        records: &records,
    };
    let ack = |status| {
        ServerControl::Ack(ServerAck {
            client_msg_id: cmid,
            status,
            server_ts: now,
        })
    };
    match state.store.accept(session_id, command).await {
        Ok(AcceptOutcome::Accepted { .. }) => {
            if forward {
                forwarder::kick(state, session_id);
            }
            CommandReply::Reply(ack(AckStatus::Accepted))
        }
        Ok(AcceptOutcome::Duplicate) => CommandReply::Reply(ack(AckStatus::Duplicate)),
        Ok(AcceptOutcome::SessionMissing) => CommandReply::SessionGone(error(
            ErrorCode::Internal,
            "la sesión expiró".into(),
            Some(cmid),
            true,
        )),
        Err(e) => {
            tracing::error!(error = %e, %session_id, "no se pudo aceptar un comando");
            CommandReply::Reply(error(
                ErrorCode::Internal,
                "no se pudo guardar; reintenta".into(),
                Some(cmid),
                true,
            ))
        }
    }
}

/// Evento de registro de un comando aceptado (con su contenido).
fn accepted_record(
    state: &AppState,
    session_id: Uuid,
    message: &ClientMessage,
) -> Option<RecordEvent> {
    let cmid = message.client_msg_id().to_string();
    let event = match message {
        ClientMessage::CreateIncident(m) => {
            RecordEvent::new(EventKind::IncidentRequested, producer(state))
                .client_ts(m.client_ts)
                .trace(m.traceparent.as_deref())
                .attr("channel", m.channel.as_str())
                .attr("language", m.language.clone())
        }
        ClientMessage::ChatMessage(m) => {
            RecordEvent::new(EventKind::CitizenMessage, producer(state))
                .client_ts(m.client_ts)
                .trace(m.traceparent.as_deref())
                .attr("kind", serde_json::to_value(m.kind).unwrap_or_default())
                .attr(
                    "quick_reply",
                    serde_json::to_value(m.quick_reply).unwrap_or_default(),
                )
                // Valkey sustituye el marcador por el seq real del eco.
                .attr("seq", SEQ_PLACEHOLDER)
                .sensitive(SensitivePayload::Message {
                    text: m.text.clone(),
                })
        }
        ClientMessage::LocationUpdate(m) => {
            RecordEvent::new(EventKind::LocationReceived, producer(state))
                .client_ts(m.client_ts)
                .trace(m.traceparent.as_deref())
                .attr("source", serde_json::to_value(m.source).unwrap_or_default())
                .sensitive(location_payload(m))
        }
        ClientMessage::VideoCallRequest(m) => {
            RecordEvent::new(EventKind::VideoCallRequested, producer(state))
                .client_ts(m.client_ts)
                .trace(m.traceparent.as_deref())
                .attr("media", m.media.unwrap_or_default().as_str())
                .attr("stage", "received")
        }
        ClientMessage::ClientEvent(m) => RecordEvent::new(EventKind::ClientEvent, producer(state))
            .client_ts(m.client_ts)
            .trace(m.traceparent.as_deref())
            .attr("event", serde_json::to_value(m.kind).unwrap_or_default())
            .attr("detail", m.detail.clone()),
        ClientMessage::Close(m) => RecordEvent::new(EventKind::ClientEvent, producer(state))
            .client_ts(m.client_ts)
            .attr("event", "close_requested")
            .attr("detail", serde_json::to_value(m.reason).unwrap_or_default()),
        ClientMessage::Hello(_) | ClientMessage::Ack(_) | ClientMessage::Ping(_) => return None,
    };
    Some(event.session(session_id).attr("client_msg_id", cmid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_rules() {
        let allowed = vec!["https://sinbarreras.zapopan.gob.mx".to_owned()];
        let with = |origin: &str| {
            let mut h = HeaderMap::new();
            h.insert(header::ORIGIN, origin.parse().unwrap());
            h
        };
        assert!(origin_allowed(&allowed, &HeaderMap::new()));
        assert!(origin_allowed(
            &allowed,
            &with("https://sinbarreras.zapopan.gob.mx")
        ));
        assert!(!origin_allowed(&allowed, &with("https://evil.example")));
        assert!(origin_allowed(
            &["*".to_owned()],
            &with("http://localhost:5173")
        ));
    }

    #[test]
    fn client_ip_prefers_rightmost_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.1.1.1, 203.0.113.9".parse().unwrap());
        let peer: SocketAddr = "10.0.0.5:4000".parse().unwrap();
        assert_eq!(client_ip(&headers, peer), "203.0.113.9");
        assert_eq!(client_ip(&HeaderMap::new(), peer), "10.0.0.5");
    }

    #[test]
    fn extracts_client_msg_id_from_invalid_message() {
        let id = Uuid::new_v4();
        let raw = format!(r#"{{"type":"desconocido","client_msg_id":"{id}"}}"#);
        assert_eq!(extract_client_msg_id(&raw), Some(id));
        assert_eq!(extract_client_msg_id("no json"), None);
    }
}
