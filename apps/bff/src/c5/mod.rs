//! Puerto hacia el C5 (arquitectura hexagonal).
//!
//! El BFF solo conoce este trait. Hoy hay dos adaptadores:
//! - [`mock_http::MockHttpGateway`]: habla con `apps/c5-mock` (API inventada).
//! - [`memory::InMemoryGateway`]: en memoria, para pruebas.
//!
//! Cuando llegue la documentación real del C5 se escribe un tercer adaptador;
//! nada más del BFF cambia. Ver `docs/c5-preguntas.md`.
//!
//! Contrato de idempotencia: toda operación recibe una `idempotency_key`
//! (nuestro `client_msg_id`). Si el BFF reintenta (timeout, cambio de réplica),
//! el adaptador debe garantizar que el C5 no lo duplique.

pub mod memory;
pub mod mock_http;

use futures::{future::BoxFuture, stream::BoxStream};
use protocol::{
    CallMedia, Channel, ChatKind, DeliveryStatus, IncidentState, QuickReply, Timestamp,
    VideoCallState,
};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum C5Error {
    /// Falla temporal (red, timeout, 5xx): se reintenta.
    #[error("falla temporal del C5: {0}")]
    Transient(String),
    /// El C5 rechazó la operación de forma definitiva (4xx): no se reintenta.
    #[error("el C5 rechazó la operación: {0}")]
    Rejected(String),
}

impl C5Error {
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Transient(_))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewIncident {
    pub idempotency_key: Uuid,
    /// Nuestro identificador del incidente (se envía al C5 para cruzar).
    pub incident_id: Uuid,
    pub channel: Channel,
    pub language: String,
    /// Punto de entrada (QR) y si su firma se verificó.
    pub entry_point: Option<String>,
    pub entry_verified: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutboundMessage {
    pub idempotency_key: Uuid,
    pub c5_incident_id: String,
    pub kind: ChatKind,
    pub text: String,
    pub quick_reply: Option<QuickReply>,
    pub client_ts: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocationReport {
    pub idempotency_key: Uuid,
    pub c5_incident_id: String,
    pub latitude: f64,
    pub longitude: f64,
    pub accuracy_m: Option<f64>,
    /// `gps`, `network` o `qr`.
    pub source: String,
    pub captured_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VideoCallRequest {
    pub idempotency_key: Uuid,
    pub c5_incident_id: String,
    /// `video` (LSM) o `audio` (solo voz).
    pub media: CallMedia,
}

/// Lo que el C5 nos empuja (mensajes del operador y cambios de estado).
#[derive(Debug, Clone, PartialEq)]
pub struct C5InboundEvent {
    /// Identificador único del evento en el C5 (para deduplicar).
    pub event_id: String,
    pub c5_incident_id: String,
    pub occurred_at: Timestamp,
    pub kind: C5InboundKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum C5InboundKind {
    OperatorMessage {
        c5_message_id: String,
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

/// Las operaciones que el BFF necesita del C5.
pub trait C5Gateway: Send + Sync + 'static {
    /// Crea el incidente. Devuelve el identificador del C5.
    fn create_incident(&self, request: NewIncident) -> BoxFuture<'_, Result<String, C5Error>>;
    /// Envía un mensaje del ciudadano. Devuelve el id del mensaje en el C5.
    fn send_message(&self, message: OutboundMessage) -> BoxFuture<'_, Result<String, C5Error>>;
    fn update_location(&self, report: LocationReport) -> BoxFuture<'_, Result<(), C5Error>>;
    /// Solicita la videollamada. Los datos para abrirla llegan después como
    /// `C5InboundKind::VideoCall`.
    fn request_video_call(
        &self,
        request: VideoCallRequest,
    ) -> BoxFuture<'_, Result<String, C5Error>>;
    /// Flujo de eventos del C5 (mensajes del operador y cambios de estado).
    /// El adaptador se encarga de reconectar; el flujo no termina mientras el
    /// proceso viva.
    fn events(&self) -> BoxStream<'static, C5InboundEvent>;
}
