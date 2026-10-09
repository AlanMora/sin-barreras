//! Simulador del C5 para desarrollo y pruebas.
//!
//! IMPORTANTE: la API que expone este simulador es INVENTADA por nosotros. NO
//! refleja la API real del C5, que aún no está documentada.
//!
//! - `/mock-api/v1/*`: lo que consume el adaptador `MockHttpGateway` del BFF.
//! - `/mock-control/*`: atajos para pruebas y demostraciones (forzar mensajes
//!   del operador, estados y fallas).

pub mod config;
pub mod operator;
pub mod state;

use std::{sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::{
        Path, Query, Request, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use protocol::{IncidentState, Timestamp};
use serde::Deserialize;
use serde_json::{Value, json};
use service_kit::health::{self, NoDependencies};
use tracing::Instrument;
use uuid::Uuid;

use crate::{
    config::{Chaos, Config},
    state::{Incident, MockState, random_between, random_unit},
};

pub fn app(config: Config) -> Router {
    let state = Arc::new(MockState::new(config));
    if state.chaos().feed_drop_every_secs > 0 {
        tracing::warn!("caos activo: el feed se cortará periódicamente");
    }
    let api = Router::new()
        .route("/incidents", post(create_incident))
        .route("/incidents/{id}/messages", post(post_message))
        .route("/incidents/{id}/locations", post(post_location))
        .route("/incidents/{id}/video-calls", post(post_video_call))
        .route("/events", get(events))
        .layer(middleware::from_fn_with_state(
            Arc::clone(&state),
            chaos_and_auth,
        ));
    let control = Router::new()
        .route("/incidents", get(list_incidents))
        .route("/incidents/{id}/operator-message", post(control_message))
        .route("/incidents/{id}/status", post(control_status))
        .route("/chaos", get(get_chaos).post(set_chaos));
    Router::new()
        .nest("/mock-api/v1", api)
        .nest("/mock-control", control)
        .with_state(state)
        .merge(health::router(NoDependencies, env!("CARGO_PKG_VERSION")))
}

/// Autenticación por `x-api-key` y fallas inyectadas.
async fn chaos_and_auth(
    State(state): State<Arc<MockState>>,
    request: Request,
    next: Next,
) -> Response {
    let expected = &state.config.api_key;
    if !expected.is_empty() {
        let given = request
            .headers()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        if given != expected {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "api key inválida" })),
            )
                .into_response();
        }
    }
    let chaos = state.chaos();
    if request.method() == axum::http::Method::POST {
        tokio::time::sleep(Duration::from_millis(random_between(
            chaos.latency_min_ms,
            chaos.latency_max_ms,
        )))
        .await;
        if random_unit() < chaos.failure_rate {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": "falla simulada" })),
            )
                .into_response();
        }
    }
    // Continúa la traza que propaga el BFF (simula el último salto: el C5).
    let traceparent = request
        .headers()
        .get("traceparent")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let span = tracing::info_span!("c5_request", path = %request.uri().path());
    service_kit::telemetry::continue_trace(&span, traceparent.as_deref());
    next.run(request).instrument(span).await
}

#[derive(Deserialize)]
struct NewIncident {
    idempotency_key: Uuid,
    external_ref: Uuid,
    channel: String,
    language: String,
    entry_point: Option<String>,
    #[serde(default)]
    entry_verified: bool,
}

async fn create_incident(
    State(state): State<Arc<MockState>>,
    Json(body): Json<NewIncident>,
) -> Response {
    let (response, fresh) = state.idempotent(
        "incident",
        body.idempotency_key,
        |ids| json!({ "incident_id": ids.incident() }),
    );
    let incident_id = response["incident_id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    if fresh {
        tracing::info!(%incident_id, channel = %body.channel, "incidente simulado creado");
        state.insert_incident(Incident {
            incident_id: incident_id.clone(),
            external_ref: body.external_ref,
            channel: body.channel.clone(),
            language: body.language.clone(),
            entry_point: body.entry_point,
            entry_verified: body.entry_verified,
            status: IncidentState::Received,
            created_at: Timestamp::now(),
            citizen_messages: 0,
            locations: Vec::new(),
        });
        operator::on_incident_created(Arc::clone(&state), incident_id, body.language, body.channel);
    }
    (StatusCode::CREATED, Json(response)).into_response()
}

#[derive(Deserialize)]
struct NewMessage {
    idempotency_key: Uuid,
    #[serde(rename = "kind")]
    _kind: String,
    #[serde(rename = "text")]
    _text: String,
    quick_reply: Option<String>,
}

async fn post_message(
    State(state): State<Arc<MockState>>,
    Path(id): Path<String>,
    Json(body): Json<NewMessage>,
) -> Response {
    let Some(language) = state.with_incident(&id, |i| i.language.clone()) else {
        return not_found();
    };
    let (response, fresh) = state.idempotent(
        "message",
        body.idempotency_key,
        |ids| json!({ "message_id": ids.message() }),
    );
    if fresh {
        let count = state
            .with_incident(&id, |i| {
                i.citizen_messages += 1;
                i.citizen_messages
            })
            .unwrap_or(1);
        operator::on_citizen_message(
            Arc::clone(&state),
            id,
            body.idempotency_key,
            body.quick_reply,
            count,
            language,
        );
    }
    (StatusCode::CREATED, Json(response)).into_response()
}

async fn post_location(
    State(state): State<Arc<MockState>>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    // La ubicación no se registra en logs: solo se guarda en memoria.
    match state.with_incident(&id, |i| i.locations.push(body)) {
        Some(()) => (StatusCode::ACCEPTED, Json(json!({}))).into_response(),
        None => not_found(),
    }
}

#[derive(Deserialize)]
struct NewVideoCall {
    idempotency_key: Uuid,
    #[serde(default = "default_media")]
    media: String,
}

fn default_media() -> String {
    "video".into()
}

async fn post_video_call(
    State(state): State<Arc<MockState>>,
    Path(id): Path<String>,
    Json(body): Json<NewVideoCall>,
) -> Response {
    if state.with_incident(&id, |_| ()).is_none() {
        return not_found();
    }
    let (response, fresh) = state.idempotent(
        "video_call",
        body.idempotency_key,
        |_| json!({ "video_call_id": format!("VID-{}", Uuid::new_v4().simple()) }),
    );
    if fresh {
        let video_call_id = response["video_call_id"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        operator::on_video_call(
            Arc::clone(&state),
            id,
            body.idempotency_key,
            video_call_id,
            body.media,
        );
    }
    (StatusCode::CREATED, Json(response)).into_response()
}

#[derive(Deserialize)]
struct Since {
    #[serde(default)]
    since: u64,
}

/// Feed de eventos: primero los pendientes desde `since`, después en vivo.
async fn events(
    State(state): State<Arc<MockState>>,
    Query(q): Query<Since>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| feed(state, socket, q.since))
}

async fn feed(state: Arc<MockState>, mut socket: WebSocket, since: u64) {
    // Suscribirse antes de leer el historial: no se pierde nada en medio.
    let mut live = state.feed.subscribe();
    let backlog = state.events_since(since);
    let mut last_sent = since;
    for event in backlog {
        last_sent = event_seq(&event).unwrap_or(last_sent);
        if socket.send(Message::Text(event.into())).await.is_err() {
            return;
        }
    }
    let drop_every = state.chaos().feed_drop_every_secs;
    let drop_at =
        (drop_every > 0).then(|| tokio::time::Instant::now() + Duration::from_secs(drop_every));
    loop {
        let next = async {
            match drop_at {
                Some(at) => tokio::time::timeout_at(at, live.recv()).await.ok(),
                None => Some(live.recv().await),
            }
        };
        match next.await {
            Some(Ok(event)) => {
                if event_seq(&event).is_some_and(|seq| seq <= last_sent) {
                    continue;
                }
                if socket.send(Message::Text(event.into())).await.is_err() {
                    return;
                }
            }
            Some(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {
                // El cliente se atrasó: se cierra y al reconectar pide `since`.
                return;
            }
            Some(Err(_)) => return,
            None => {
                tracing::info!("caos: cortando feed");
                let _ = socket.send(Message::Close(None)).await;
                return;
            }
        }
    }
}

fn event_seq(event: &str) -> Option<u64> {
    serde_json::from_str::<Value>(event)
        .ok()?
        .get("event_seq")?
        .as_u64()
}

// --- Control ---------------------------------------------------------------

async fn list_incidents(State(state): State<Arc<MockState>>) -> Json<Value> {
    Json(json!(state.incidents()))
}

#[derive(Deserialize)]
struct ControlMessage {
    text: String,
}

async fn control_message(
    State(state): State<Arc<MockState>>,
    Path(id): Path<String>,
    Json(body): Json<ControlMessage>,
) -> Response {
    if state.with_incident(&id, |_| ()).is_none() {
        return not_found();
    }
    operator::say(&state, &id, &body.text);
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Deserialize)]
struct ControlStatus {
    status: IncidentState,
    eta_seconds: Option<u32>,
}

async fn control_status(
    State(state): State<Arc<MockState>>,
    Path(id): Path<String>,
    Json(body): Json<ControlStatus>,
) -> Response {
    if state.with_incident(&id, |_| ()).is_none() {
        return not_found();
    }
    operator::set_status(&state, &id, body.status, body.eta_seconds);
    StatusCode::NO_CONTENT.into_response()
}

async fn get_chaos(State(state): State<Arc<MockState>>) -> Json<Chaos> {
    Json(state.chaos())
}

async fn set_chaos(State(state): State<Arc<MockState>>, Json(chaos): Json<Chaos>) -> Json<Chaos> {
    if let Ok(mut current) = state.chaos.lock() {
        *current = chaos;
    }
    tracing::warn!(?chaos, "caos actualizado");
    Json(chaos)
}

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "incidente no encontrado" })),
    )
        .into_response()
}
