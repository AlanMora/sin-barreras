//! Mensajes del servidor (BFF) al cliente (PWA).
//!
//! Hay dos familias:
//! - **Control** (`welcome`, `ack`, `pong`, `error`): respuestas inmediatas a
//!   la conexión actual. No llevan `seq` y no se reenvían al reconectar.
//! - **Secuenciados** (`SequencedMessage`): todo lo que forma parte de la
//!   conversación o del estado del incidente. Llevan un `seq` incremental por
//!   sesión, el cliente los confirma con `ack { seq }` y, al reconectar, el
//!   BFF reenvía todos los posteriores al `last_seq` del cliente.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::{Channel, ChatKind, QuickReply, Timestamp};

// ---------------------------------------------------------------------------
// Control
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerControl {
    Welcome(Welcome),
    Ack(ServerAck),
    Pong(Pong),
    Error(ServerError),
}

/// Resultado de la verificación del QR de entrada (ver ADR-0007: un QR no
/// verificado NO impide pedir ayuda).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum EntryVerification {
    /// Sin QR (entrada directa por web).
    None,
    Verified,
    Unverified,
}

impl EntryVerification {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Verified => "verified",
            Self::Unverified => "unverified",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Welcome {
    pub protocol_version: u16,
    pub session_id: Uuid,
    /// Token anónimo para reanudar. Cambia en cada `welcome`: el anterior
    /// queda invalidado.
    pub session_token: String,
    pub token_expires_at: Timestamp,
    /// `true` si se reanudó la sesión pedida en `hello.resume`.
    pub resumed: bool,
    pub entry: EntryVerification,
    pub heartbeat_interval_ms: u32,
    /// Último `seq` emitido en la sesión. Si es mayor que el `last_seq` del
    /// cliente, a continuación llegan los mensajes faltantes.
    pub latest_seq: u64,
    #[ts(optional)]
    pub incident: Option<IncidentSummary>,
    pub server_ts: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct IncidentSummary {
    pub incident_id: Uuid,
    pub channel: Channel,
    pub status: IncidentState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AckStatus {
    /// Recibido y guardado de forma durable por primera vez.
    Accepted,
    /// Ya se había recibido (reenvío por reconexión); no se duplicó.
    Duplicate,
}

/// Confirmación de un mensaje del cliente: el indicador "enviado".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ServerAck {
    pub client_msg_id: Uuid,
    pub status: AckStatus,
    pub server_ts: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Pong {
    pub client_msg_id: Uuid,
    pub server_ts: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// La versión del cliente no es compatible. Ver `supported_versions`.
    UnsupportedProtocolVersion,
    /// Se envió un mensaje antes de `hello`.
    HelloRequired,
    /// JSON mal formado, tipo desconocido o campo inválido.
    InvalidMessage,
    MessageTooLarge,
    /// Demasiados mensajes; reintentar más tarde (nunca aplica a la
    /// creación del incidente ni al primer mensaje de ayuda).
    RateLimited,
    /// Falla interna temporal; el cliente debe reintentar con el mismo
    /// `client_msg_id`.
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ServerError {
    pub code: ErrorCode,
    /// Descripción técnica breve (no se muestra tal cual al ciudadano; la PWA
    /// traduce el `code`).
    pub message: String,
    #[ts(optional)]
    pub client_msg_id: Option<Uuid>,
    /// Si el cliente puede reintentar el mismo mensaje.
    pub retryable: bool,
    #[ts(optional)]
    pub supported_versions: Option<Vec<u16>>,
    pub server_ts: Timestamp,
}

// ---------------------------------------------------------------------------
// Secuenciados
// ---------------------------------------------------------------------------

/// Mensaje secuenciado tal como viaja por el WebSocket: `seq` + evento.
///
/// El BFF nunca construye este tipo para enviarlo: el `seq` lo asigna Valkey
/// de forma atómica y antepone `"seq":N` al JSON del `ServerEvent`. Este tipo
/// existe para describir el formato (TypeScript) y para las pruebas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SequencedMessage {
    pub seq: u64,
    #[serde(flatten)]
    pub event: ServerEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    IncidentCreated(IncidentCreated),
    ChatMessage(ServerChatMessage),
    MessageStatus(MessageStatus),
    IncidentStatus(IncidentStatus),
    VideoCall(VideoCallUpdate),
    SessionClosed(SessionClosed),
}

impl ServerEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::IncidentCreated(_) => "incident_created",
            Self::ChatMessage(_) => "chat_message",
            Self::MessageStatus(_) => "message_status",
            Self::IncidentStatus(_) => "incident_status",
            Self::VideoCall(_) => "video_call",
            Self::SessionClosed(_) => "session_closed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct IncidentCreated {
    /// Nuestro identificador del incidente (no el del C5).
    pub incident_id: Uuid,
    pub channel: Channel,
    /// `client_msg_id` del `create_incident` (o del mensaje que lo provocó).
    pub client_msg_id: Uuid,
    pub server_ts: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Author {
    Citizen,
    Operator,
    System,
}

/// Mensaje de la conversación. Los del ciudadano también se emiten (eco con
/// su `client_msg_id`) para que la conversación completa se pueda
/// reconstruir desde el servidor tras una recarga.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ServerChatMessage {
    pub message_id: Uuid,
    pub author: Author,
    pub kind: ChatKind,
    pub text: String,
    #[ts(optional)]
    pub quick_reply: Option<QuickReply>,
    #[ts(optional)]
    pub client_msg_id: Option<Uuid>,
    pub server_ts: Timestamp,
}

/// Estado de entrega de un mensaje del ciudadano al operador.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    /// El C5 lo recibió.
    Delivered,
    /// El operador lo leyó.
    Read,
    /// El C5 lo rechazó de forma definitiva (se muestra al ciudadano con la
    /// opción de llamar al número de respaldo).
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MessageStatus {
    pub client_msg_id: Uuid,
    pub status: DeliveryStatus,
    pub server_ts: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum IncidentState {
    /// Recibido por el C5, esperando operador.
    Received,
    /// "Te estamos atendiendo".
    Attending,
    /// "Unidad en camino" (con tiempo estimado si existe).
    UnitEnRoute,
    /// La unidad llegó al lugar.
    UnitOnScene,
    /// "Incidente cerrado".
    Closed,
}

impl IncidentState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Received => "received",
            Self::Attending => "attending",
            Self::UnitEnRoute => "unit_en_route",
            Self::UnitOnScene => "unit_on_scene",
            Self::Closed => "closed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct IncidentStatus {
    pub status: IncidentState,
    /// Tiempo estimado de llegada, en segundos.
    #[ts(optional)]
    pub eta_seconds: Option<u32>,
    pub server_ts: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum VideoCallState {
    /// El C5 recibió la solicitud.
    Requested,
    /// Lista para abrir: `join` trae los datos del componente de video.
    Ready,
    Failed,
    Ended,
}

/// Punto de integración con el sistema de videollamadas del otro equipo.
/// `join` es opaco para nosotros: se pasa tal cual al componente de video.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct VideoCallUpdate {
    pub request_client_msg_id: Uuid,
    pub status: VideoCallState,
    #[ts(optional, type = "Record<string, unknown>")]
    pub join: Option<serde_json::Value>,
    pub server_ts: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SessionClosedReason {
    IncidentClosed,
    EndedByCitizen,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SessionClosed {
    pub reason: SessionClosedReason,
    pub server_ts: Timestamp,
}

/// Cualquier frame que el servidor puede enviar (útil para clientes Rust y
/// pruebas). Se distingue por la presencia de `seq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ServerFrame {
    Sequenced(SequencedMessage),
    Control(ServerControl),
}
