//! Mensajes del cliente (PWA) al servidor (BFF).
//!
//! Reglas comunes a todos los mensajes del cliente:
//! - `client_msg_id` (UUID) único por mensaje; un reenvío por reconexión usa
//!   el mismo id y el BFF no lo duplica.
//! - `client_ts`: hora del dispositivo (puede estar mal; el registro guarda
//!   también la hora del servidor).
//! - `traceparent`: contexto W3C opcional para correlacionar trazas.
//! - Campos desconocidos se rechazan (`deny_unknown_fields`).

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::Timestamp;

/// Todo lo que el cliente puede enviar. El campo `type` identifica la variante.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Hello(Hello),
    CreateIncident(CreateIncident),
    ChatMessage(ClientChatMessage),
    LocationUpdate(LocationUpdate),
    VideoCallRequest(VideoCallRequest),
    ClientEvent(ClientEvent),
    Ack(ClientAck),
    Ping(Ping),
    Close(ClientClose),
}

impl ClientMessage {
    pub fn client_msg_id(&self) -> Uuid {
        match self {
            Self::Hello(m) => m.client_msg_id,
            Self::CreateIncident(m) => m.client_msg_id,
            Self::ChatMessage(m) => m.client_msg_id,
            Self::LocationUpdate(m) => m.client_msg_id,
            Self::VideoCallRequest(m) => m.client_msg_id,
            Self::ClientEvent(m) => m.client_msg_id,
            Self::Ack(m) => m.client_msg_id,
            Self::Ping(m) => m.client_msg_id,
            Self::Close(m) => m.client_msg_id,
        }
    }

    /// Nombre estable del tipo de mensaje (para métricas y logs).
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Hello(_) => "hello",
            Self::CreateIncident(_) => "create_incident",
            Self::ChatMessage(_) => "chat_message",
            Self::LocationUpdate(_) => "location_update",
            Self::VideoCallRequest(_) => "video_call_request",
            Self::ClientEvent(_) => "client_event",
            Self::Ack(_) => "ack",
            Self::Ping(_) => "ping",
            Self::Close(_) => "close",
        }
    }
}

/// Primer mensaje de toda conexión. Negocia versión y crea o reanuda sesión.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Hello {
    pub client_msg_id: Uuid,
    #[ts(optional)]
    pub client_ts: Option<Timestamp>,
    #[ts(optional)]
    pub traceparent: Option<String>,
    /// Versión del protocolo que habla el cliente.
    pub protocol_version: u16,
    /// Versión de la PWA (`0.1.0`).
    pub app_version: String,
    /// Idioma de la interfaz (BCP 47, p. ej. `es-MX`).
    pub language: String,
    /// Presente si el cliente quiere reanudar una sesión previa.
    #[ts(optional)]
    pub resume: Option<Resume>,
    /// Punto de entrada por QR (`/?t=TOT-0142&k=1&s=...`).
    #[ts(optional)]
    pub entry: Option<QrEntry>,
    /// Ajustes de accesibilidad activos (se registran como metadatos).
    #[ts(optional)]
    pub accessibility: Option<AccessibilitySettings>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Resume {
    pub session_id: Uuid,
    pub session_token: String,
    /// Último `seq` recibido y procesado; el BFF reenvía todo lo posterior.
    pub last_seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct QrEntry {
    /// Identificador del punto (`TOT-0142`).
    pub point_id: String,
    /// Identificador de la llave con la que se firmó el QR.
    #[ts(optional)]
    pub key_id: Option<String>,
    /// Firma HMAC del QR, base64url.
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AccessibilitySettings {
    /// Escala de letra (1.0 = normal).
    pub font_scale: f32,
    pub high_contrast: bool,
    pub reduced_motion: bool,
    pub silent_mode: bool,
}

/// Canal con el que el ciudadano eligió comunicarse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    /// Videollamada en Lengua de Señas Mexicana.
    LsmVideo,
    TextChat,
    Audio,
    /// Chat con traducción a otro idioma.
    Translation,
    /// Modo silencioso: sin sonido ni vibración, respuestas rápidas.
    Silent,
}

impl Channel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LsmVideo => "lsm_video",
            Self::TextChat => "text_chat",
            Self::Audio => "audio",
            Self::Translation => "translation",
            Self::Silent => "silent",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CreateIncident {
    pub client_msg_id: Uuid,
    #[ts(optional)]
    pub client_ts: Option<Timestamp>,
    #[ts(optional)]
    pub traceparent: Option<String>,
    pub channel: Channel,
    /// Idioma en que el ciudadano se comunicará (BCP 47). Relevante para
    /// el canal `translation`.
    #[ts(optional)]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ChatKind {
    Text,
    QuickReply,
}

/// Respuestas rápidas del modo silencioso.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum QuickReply {
    InDanger,
    CannotSpeak,
    BeingFollowed,
    Injured,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ClientChatMessage {
    pub client_msg_id: Uuid,
    #[ts(optional)]
    pub client_ts: Option<Timestamp>,
    #[ts(optional)]
    pub traceparent: Option<String>,
    pub kind: ChatKind,
    /// Texto tal como lo ve el ciudadano (en respuestas rápidas, el texto
    /// localizado del botón).
    pub text: String,
    /// Código de la respuesta rápida; obligatorio si `kind = quick_reply`.
    #[ts(optional)]
    pub quick_reply: Option<QuickReply>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum LocationSource {
    Gps,
    Network,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct LocationUpdate {
    pub client_msg_id: Uuid,
    #[ts(optional)]
    pub client_ts: Option<Timestamp>,
    #[ts(optional)]
    pub traceparent: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    /// Precisión reportada por el dispositivo, en metros.
    #[ts(optional)]
    pub accuracy_m: Option<f64>,
    pub source: LocationSource,
    /// Momento en que el dispositivo obtuvo la posición.
    pub captured_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct VideoCallRequest {
    pub client_msg_id: Uuid,
    #[ts(optional)]
    pub client_ts: Option<Timestamp>,
    #[ts(optional)]
    pub traceparent: Option<String>,
}

/// Eventos de interfaz que deben quedar en el registro aunque no vayan al C5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ClientEventKind {
    AppOpened,
    PrivacyNoticeShown,
    PrivacyNoticeOpened,
    ChannelScreenShown,
    /// El ciudadano salió sin pedir ayuda.
    AbandonedBeforeHelp,
    /// Botón de salida rápida del modo silencioso (la sesión sigue viva).
    QuickExitUsed,
    SettingsChanged,
    PermissionGranted,
    PermissionDenied,
    WentOffline,
    CameBackOnline,
    ClientError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ClientEvent {
    pub client_msg_id: Uuid,
    #[ts(optional)]
    pub client_ts: Option<Timestamp>,
    #[ts(optional)]
    pub traceparent: Option<String>,
    pub kind: ClientEventKind,
    /// Detalle breve y no sensible (p. ej. `geolocation`, `font_scale=1.3`).
    #[ts(optional)]
    pub detail: Option<String>,
}

/// Confirma la recepción de un mensaje del servidor con `seq`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ClientAck {
    pub client_msg_id: Uuid,
    #[ts(optional)]
    pub client_ts: Option<Timestamp>,
    pub seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Ping {
    pub client_msg_id: Uuid,
    #[ts(optional)]
    pub client_ts: Option<Timestamp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ClientCloseReason {
    /// El ciudadano terminó la conversación de forma explícita.
    EndedByCitizen,
    /// Salió antes de pedir ayuda.
    AbandonedBeforeHelp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ClientClose {
    pub client_msg_id: Uuid,
    #[ts(optional)]
    pub client_ts: Option<Timestamp>,
    pub reason: ClientCloseReason,
}
