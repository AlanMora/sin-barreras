//! Adaptador hacia el simulador `apps/c5-mock`.
//!
//! IMPORTANTE: la API que consume este adaptador es INVENTADA por nosotros y
//! no representa la del C5. Sirve para desarrollar y probar el BFF mientras
//! llega la documentación real.

use std::time::Duration;

use futures::{FutureExt, SinkExt, StreamExt, future::BoxFuture, stream::BoxStream};
use protocol::{ChatKind, DeliveryStatus, IncidentState, QuickReply, Timestamp, VideoCallState};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use service_kit::backoff::Backoff;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest, http::HeaderValue};
use uuid::Uuid;

use super::{
    C5Error, C5Gateway, C5InboundEvent, C5InboundKind, LocationReport, NewIncident,
    OutboundMessage, VideoCallRequest,
};
use crate::config::C5Config;

const API_KEY_HEADER: &str = "x-api-key";

pub struct MockHttpGateway {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl MockHttpGateway {
    pub fn new(config: &C5Config) -> Result<Self, reqwest::Error> {
        let http = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .connect_timeout(Duration::from_secs(3))
            .build()?;
        Ok(Self {
            http,
            base_url: config.mock_url.trim_end_matches('/').to_owned(),
            api_key: config.mock_api_key.clone(),
        })
    }

    async fn post<B: Serialize, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<R, C5Error> {
        let response = self
            .http
            .post(format!("{}{path}", self.base_url))
            .header(API_KEY_HEADER, &self.api_key)
            .json(body)
            .send()
            .await
            .map_err(|e| C5Error::Transient(format!("red: {}", e.without_url())))?;
        let status = response.status();
        if status.is_success() {
            return response
                .json::<R>()
                .await
                .map_err(|e| C5Error::Transient(format!("respuesta: {e}")));
        }
        Err(classify(status))
    }
}

fn classify(status: StatusCode) -> C5Error {
    if status.is_server_error()
        || status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::REQUEST_TIMEOUT
    {
        C5Error::Transient(format!("HTTP {status}"))
    } else {
        C5Error::Rejected(format!("HTTP {status}"))
    }
}

// --- DTOs de la API simulada ----------------------------------------------

#[derive(Serialize)]
struct CreateIncidentBody<'a> {
    idempotency_key: Uuid,
    external_ref: Uuid,
    channel: &'a str,
    language: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    entry_point: Option<&'a str>,
    entry_verified: bool,
}

#[derive(Deserialize)]
struct CreatedIncident {
    incident_id: String,
}

#[derive(Serialize)]
struct MessageBody<'a> {
    idempotency_key: Uuid,
    kind: ChatKind,
    text: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    quick_reply: Option<QuickReply>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_ts: Option<Timestamp>,
}

#[derive(Deserialize)]
struct CreatedMessage {
    message_id: String,
}

#[derive(Serialize)]
struct LocationBody<'a> {
    idempotency_key: Uuid,
    latitude: f64,
    longitude: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    accuracy_m: Option<f64>,
    source: &'a str,
    captured_at: Timestamp,
}

#[derive(Serialize)]
struct VideoCallBody {
    idempotency_key: Uuid,
    media: &'static str,
}

#[derive(Deserialize)]
struct CreatedVideoCall {
    video_call_id: String,
}

#[derive(Deserialize)]
struct Empty {}

/// Evento del feed del simulador.
#[derive(Debug, Deserialize)]
struct FeedEvent {
    event_seq: u64,
    event_id: String,
    incident_id: String,
    occurred_at: Timestamp,
    #[serde(flatten)]
    body: FeedBody,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum FeedBody {
    OperatorMessage {
        message_id: String,
        text: String,
    },
    MessageStatus {
        idempotency_key: Uuid,
        status: DeliveryStatus,
    },
    IncidentStatus {
        status: IncidentState,
        eta_seconds: Option<u32>,
    },
    VideoCall {
        idempotency_key: Uuid,
        status: VideoCallState,
        join: Option<serde_json::Value>,
    },
}

impl From<FeedEvent> for C5InboundEvent {
    fn from(e: FeedEvent) -> Self {
        let kind = match e.body {
            FeedBody::OperatorMessage { message_id, text } => C5InboundKind::OperatorMessage {
                c5_message_id: message_id,
                text,
            },
            FeedBody::MessageStatus {
                idempotency_key,
                status,
            } => C5InboundKind::MessageStatus {
                idempotency_key,
                status,
            },
            FeedBody::IncidentStatus {
                status,
                eta_seconds,
            } => C5InboundKind::IncidentStatus {
                status,
                eta_seconds,
            },
            FeedBody::VideoCall {
                idempotency_key,
                status,
                join,
            } => C5InboundKind::VideoCall {
                idempotency_key,
                status,
                join,
            },
        };
        Self {
            event_id: e.event_id,
            c5_incident_id: e.incident_id,
            occurred_at: e.occurred_at,
            kind,
        }
    }
}

impl C5Gateway for MockHttpGateway {
    fn create_incident(&self, request: NewIncident) -> BoxFuture<'_, Result<String, C5Error>> {
        async move {
            let body = CreateIncidentBody {
                idempotency_key: request.idempotency_key,
                external_ref: request.incident_id,
                channel: request.channel.as_str(),
                language: &request.language,
                entry_point: request.entry_point.as_deref(),
                entry_verified: request.entry_verified,
            };
            let created: CreatedIncident = self.post("/mock-api/v1/incidents", &body).await?;
            Ok(created.incident_id)
        }
        .boxed()
    }

    fn send_message(&self, message: OutboundMessage) -> BoxFuture<'_, Result<String, C5Error>> {
        async move {
            let body = MessageBody {
                idempotency_key: message.idempotency_key,
                kind: message.kind,
                text: &message.text,
                quick_reply: message.quick_reply,
                client_ts: message.client_ts,
            };
            let path = format!("/mock-api/v1/incidents/{}/messages", message.c5_incident_id);
            let created: CreatedMessage = self.post(&path, &body).await?;
            Ok(created.message_id)
        }
        .boxed()
    }

    fn update_location(&self, report: LocationReport) -> BoxFuture<'_, Result<(), C5Error>> {
        async move {
            let body = LocationBody {
                idempotency_key: report.idempotency_key,
                latitude: report.latitude,
                longitude: report.longitude,
                accuracy_m: report.accuracy_m,
                source: &report.source,
                captured_at: report.captured_at,
            };
            let path = format!("/mock-api/v1/incidents/{}/locations", report.c5_incident_id);
            let _: Empty = self.post(&path, &body).await?;
            Ok(())
        }
        .boxed()
    }

    fn request_video_call(
        &self,
        request: VideoCallRequest,
    ) -> BoxFuture<'_, Result<String, C5Error>> {
        async move {
            let path = format!(
                "/mock-api/v1/incidents/{}/video-calls",
                request.c5_incident_id
            );
            let created: CreatedVideoCall = self
                .post(
                    &path,
                    &VideoCallBody {
                        idempotency_key: request.idempotency_key,
                        media: request.media.as_str(),
                    },
                )
                .await?;
            Ok(created.video_call_id)
        }
        .boxed()
    }

    fn events(&self) -> BoxStream<'static, C5InboundEvent> {
        let (tx, rx) = mpsc::channel(1024);
        let ws_base = self.base_url.replacen("http", "ws", 1);
        let api_key = self.api_key.clone();
        tokio::spawn(run_feed(ws_base, api_key, tx));
        futures::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|e| (e, rx)) })
            .boxed()
    }
}

/// Mantiene la conexión al feed de eventos, reconectando con backoff y
/// pidiendo los eventos desde el último recibido (`since`). Los duplicados
/// que esto pueda producir los descarta el BFF por `event_id`.
async fn run_feed(ws_base: String, api_key: String, tx: mpsc::Sender<C5InboundEvent>) {
    let mut cursor: u64 = 0;
    let mut backoff = Backoff::new(Duration::from_millis(250), Duration::from_secs(10));
    loop {
        if tx.is_closed() {
            return;
        }
        let url = format!("{ws_base}/mock-api/v1/events?since={cursor}");
        match connect(&url, &api_key).await {
            Ok(mut socket) => {
                tracing::info!("conectado al feed de eventos del C5 (simulado)");
                backoff.reset();
                while let Some(message) = socket.next().await {
                    match message {
                        Ok(Message::Text(text)) => match serde_json::from_str::<FeedEvent>(&text) {
                            Ok(event) => {
                                cursor = cursor.max(event.event_seq);
                                if tx.send(event.into()).await.is_err() {
                                    return;
                                }
                            }
                            Err(error) => {
                                tracing::warn!(%error, "evento del C5 con formato inesperado")
                            }
                        },
                        Ok(Message::Ping(payload)) => {
                            let _ = socket.send(Message::Pong(payload)).await;
                        }
                        Ok(Message::Close(_)) | Err(_) => break,
                        Ok(_) => {}
                    }
                }
                tracing::warn!("feed de eventos del C5 desconectado; reconectando");
            }
            Err(error) => tracing::warn!(%error, "no se pudo conectar al feed del C5"),
        }
        tokio::time::sleep(backoff.next_delay()).await;
    }
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn connect(
    url: &str,
    api_key: &str,
) -> Result<Socket, tokio_tungstenite::tungstenite::Error> {
    let mut request = url.into_client_request()?;
    if let Ok(value) = HeaderValue::from_str(api_key) {
        request.headers_mut().insert(API_KEY_HEADER, value);
    }
    let (socket, _) = tokio_tungstenite::connect_async(request).await?;
    Ok(socket)
}
