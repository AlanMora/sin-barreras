//! Transporte HTTP de respaldo (ADR-0018).
//!
//! Algunas redes (proxies corporativos, ciertos operadores) bloquean
//! WebSocket. En una emergencia no puede haber un único camino, así que el
//! mismo protocolo viaja también por HTTP:
//!
//! - `POST /api/v1/hello` — cuerpo: el mismo `hello`. Respuesta: `welcome` y los
//!   mensajes secuenciados que falten.
//! - `POST /api/v1/sync` — envía comandos pendientes y espera (long-polling,
//!   hasta 25 s) mensajes nuevos con `seq > last_seq`.
//!
//! Ambos usan exactamente la misma aceptación durable, idempotencia, registro
//! y `seq` que el WebSocket (`session.rs`).

use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use events::{EventKind, RecordEvent};
use protocol::{ErrorCode, PROTOCOL_VERSION, ServerControl};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::{
    AppState,
    forwarder::producer,
    session::{self, CommandReply, NetworkInfo, error},
    token,
};

/// Espera máxima de un long-poll (menor que los timeouts habituales de proxies).
const MAX_WAIT: Duration = Duration::from_secs(25);
/// Comandos máximos por petición.
const MAX_COMMANDS: usize = 50;
/// Tamaño máximo del cuerpo de una petición.
pub const MAX_BODY_BYTES: usize = 256 * 1024;

#[derive(Serialize)]
struct HelloResponse {
    welcome: ServerControl,
    frames: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncRequest {
    session_id: Uuid,
    session_token: String,
    last_seq: u64,
    #[serde(default)]
    commands: Vec<Value>,
    #[serde(default)]
    wait_ms: Option<u64>,
}

#[derive(Serialize)]
struct SyncResponse {
    replies: Vec<ServerControl>,
    frames: Vec<Value>,
    latest_seq: u64,
}

fn reply(status: StatusCode, control: ServerControl) -> Response {
    (status, Json(control)).into_response()
}

pub async fn hello(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: String,
) -> Response {
    if !session::origin_allowed(&state.config.allowed_origins, &headers) {
        return (StatusCode::FORBIDDEN, "origen no permitido").into_response();
    }
    let (hello, dropped) = match session::parse_hello(&body) {
        Ok(h) => h,
        Err((code, message, id)) => {
            return reply(StatusCode::BAD_REQUEST, error(code, message, id, false));
        }
    };
    if hello.protocol_version != PROTOCOL_VERSION {
        return reply(
            StatusCode::UPGRADE_REQUIRED,
            session::unsupported_version(&hello),
        );
    }
    let network = NetworkInfo::from_request(&headers, peer);
    let established = match session::establish_session(&state, &hello, &dropped, &network).await {
        Ok(e) => e,
        Err(e) if e.downcast_ref::<session::RateLimitedError>().is_some() => {
            let err = error(
                ErrorCode::RateLimited,
                "demasiadas sesiones; reintenta".into(),
                Some(hello.client_msg_id),
                true,
            );
            return reply(StatusCode::TOO_MANY_REQUESTS, err);
        }
        Err(e) => {
            tracing::error!(error = %e, "no se pudo establecer la sesión (HTTP)");
            let err = error(
                ErrorCode::Internal,
                "servicio no disponible".into(),
                Some(hello.client_msg_id),
                true,
            );
            return reply(StatusCode::SERVICE_UNAVAILABLE, err);
        }
    };
    let welcome = match session::welcome(&state, &established).await {
        Ok(w) => w,
        Err(_) => {
            let err = error(
                ErrorCode::Internal,
                "servicio no disponible".into(),
                None,
                true,
            );
            return reply(StatusCode::SERVICE_UNAVAILABLE, err);
        }
    };
    let frames = frames_after(&state, established.session_id, established.replay_after).await;
    state.records.record(
        RecordEvent::new(EventKind::ConnectionOpened, producer(&state))
            .session(established.session_id)
            .attr("transport", "http")
            .attr("resumed", established.resumed)
            .attr("client_last_seq", established.replay_after),
    );
    Json(HelloResponse {
        welcome: ServerControl::Welcome(welcome),
        frames,
    })
    .into_response()
}

pub async fn sync(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<SyncRequest>,
) -> Response {
    if !session::origin_allowed(&state.config.allowed_origins, &headers) {
        return (StatusCode::FORBIDDEN, "origen no permitido").into_response();
    }
    let session_id = request.session_id;
    if !token_matches(&state, session_id, &request.session_token).await {
        // El cliente debe hacer `hello` de nuevo (sesión expirada o token rotado).
        let err = error(
            ErrorCode::HelloRequired,
            "sesión no válida".into(),
            None,
            true,
        );
        return reply(StatusCode::UNAUTHORIZED, err);
    }
    if request.commands.len() > MAX_COMMANDS {
        let err = error(
            ErrorCode::MessageTooLarge,
            "demasiados comandos".into(),
            None,
            false,
        );
        return reply(StatusCode::PAYLOAD_TOO_LARGE, err);
    }

    let mut replies = Vec::new();
    for command in &request.commands {
        match session::handle_command(&state, session_id, &command.to_string()).await {
            CommandReply::Reply(r) => replies.push(r),
            CommandReply::Nothing => {}
            CommandReply::SessionGone(r) => {
                replies.push(r);
                return reply(
                    StatusCode::UNAUTHORIZED,
                    error(
                        ErrorCode::HelloRequired,
                        "sesión expirada".into(),
                        None,
                        true,
                    ),
                );
            }
        }
    }
    let _ = state.store.touch(session_id).await;

    // Long-poll: registrarse ANTES de leer para no perder nada entre medias.
    let (_registration, mut live) = state.hub.register(session_id);
    let mut latest = state.store.latest_seq(session_id).await.unwrap_or(0);
    let wait = Duration::from_millis(request.wait_ms.unwrap_or(0)).min(MAX_WAIT);
    if latest <= request.last_seq && replies.is_empty() && !wait.is_zero() {
        if let Ok(Some(frame)) = tokio::time::timeout(wait, live.recv()).await {
            latest = latest.max(frame.seq);
        }
        latest = latest.max(state.store.latest_seq(session_id).await.unwrap_or(latest));
    }
    let frames = frames_after(&state, session_id, request.last_seq).await;
    Json(SyncResponse {
        replies,
        frames,
        latest_seq: latest,
    })
    .into_response()
}

async fn frames_after(state: &AppState, session_id: Uuid, after: u64) -> Vec<Value> {
    state
        .store
        .replay(session_id, after)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(_, frame)| serde_json::from_str(&frame).ok())
        .collect()
}

/// Compara en tiempo constante el hash del token presentado con el guardado.
async fn token_matches(state: &AppState, session_id: Uuid, presented: &str) -> bool {
    let Ok(fields) = state.store.session_fields(session_id).await else {
        return false;
    };
    let Some(stored) = fields.get("token_hash") else {
        return false;
    };
    if fields.get("closed").map(String::as_str) == Some("1") {
        return false;
    }
    let presented = token::hash(presented);
    stored.as_bytes().ct_eq(presented.as_bytes()).into()
}
