//! Eventos del registro completo del lado ciudadano.
//!
//! Todo lo que pasa por el BFF, en ambas direcciones, se describe como un
//! [`RecordEvent`]. El BFF lo publica en Valkey Streams y `record-writer` lo
//! guarda en Postgres de forma permanente, append-only y encadenada.
//!
//! Separación clave: `attributes` lleva solo datos NO sensibles (canal,
//! estados, ids, versiones); `sensitive` lleva el contenido de mensajes, la
//! ubicación exacta y los datos de red (IP, user agent). `sensitive` se cifra
//! con AES-256-GCM dentro del BFF antes de salir del proceso (Fase 2): ni
//! Valkey ni Postgres lo ven en claro.
//!
//! En el stream `sb:rec:events` cada entrada lleva un [`StreamRecord`]: el
//! evento sin `sensitive` y, aparte, el contenido sellado ([`SealedPayload`]).

use protocol::Timestamp;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Versión del esquema del sobre. Se incrementa ante cambios incompatibles.
pub const SCHEMA_VERSION: u16 = 1;

/// Stream de Valkey con los eventos pendientes de guardar.
pub const RECORD_STREAM: &str = "sb:rec:events";
/// Cola de errores: eventos que fallaron repetidamente.
pub const DEAD_LETTER_STREAM: &str = "sb:rec:dlq";
/// Consumer group de record-writer.
pub const RECORD_GROUP: &str = "record-writers";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordEvent {
    /// Identificador único (UUIDv7). Es la llave de idempotencia en Postgres.
    pub event_id: Uuid,
    pub schema_version: u16,
    pub kind: EventKind,
    /// Hora del servidor (UTC, milisegundos).
    pub occurred_at: Timestamp,
    /// Hora del dispositivo, cuando aplica.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_ts: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incident_id: Option<Uuid>,
    /// Identificador del incidente en el C5, para cruzar registros.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub c5_incident_id: Option<String>,
    /// Quién produjo el evento: `bff/<réplica>`.
    pub producer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    /// Datos no sensibles.
    #[serde(default)]
    pub attributes: serde_json::Map<String, serde_json::Value>,
    /// Datos sensibles; se cifran antes de salir del BFF.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensitive: Option<SensitivePayload>,
}

impl RecordEvent {
    pub fn new(kind: EventKind, producer: impl Into<String>) -> Self {
        Self {
            event_id: Uuid::now_v7(),
            schema_version: SCHEMA_VERSION,
            kind,
            occurred_at: Timestamp::now(),
            client_ts: None,
            session_id: None,
            incident_id: None,
            c5_incident_id: None,
            producer: producer.into(),
            trace_id: None,
            attributes: serde_json::Map::new(),
            sensitive: None,
        }
    }

    pub fn session(mut self, session_id: Uuid) -> Self {
        self.session_id = Some(session_id);
        self
    }

    pub fn incident(mut self, incident_id: Option<Uuid>, c5_incident_id: Option<String>) -> Self {
        self.incident_id = incident_id;
        self.c5_incident_id = c5_incident_id;
        self
    }

    pub fn client_ts(mut self, client_ts: Option<Timestamp>) -> Self {
        self.client_ts = client_ts;
        self
    }

    pub fn trace(mut self, traceparent: Option<&str>) -> Self {
        // traceparent = 00-<trace-id>-<parent-id>-<flags>
        self.trace_id = traceparent
            .and_then(|t| t.split('-').nth(1))
            .map(str::to_owned);
        self
    }

    pub fn attr(mut self, key: &str, value: impl Into<serde_json::Value>) -> Self {
        self.attributes.insert(key.to_owned(), value.into());
        self
    }

    pub fn sensitive(mut self, payload: SensitivePayload) -> Self {
        self.sensitive = Some(payload);
        self
    }
}

/// Catálogo de eventos. Ver `docs/modelo-datos.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    // Conexión y sesión
    /// DEK de la sesión envuelta para la llave de investigación (no sensible).
    SessionKeyCreated,
    ConnectionOpened,
    ConnectionClosed,
    SessionStarted,
    SessionResumed,
    SessionResumeRejected,
    SessionClosed,
    /// Metadatos técnicos del `hello`: idioma, app, protocolo, accesibilidad, red.
    ClientMetadata,
    /// Punto de entrada (QR/tótem/web) y resultado de la verificación.
    EntryPoint,
    /// Eventos de interfaz: apertura, aviso de privacidad, abandono, etc.
    ClientEvent,
    // Incidente
    IncidentRequested,
    IncidentCreated,
    IncidentStatusChanged,
    FirstOperatorResponse,
    // Mensajes
    CitizenMessage,
    CitizenMessageForwarded,
    CitizenMessageFailed,
    OperatorMessage,
    MessageStatusChanged,
    /// El ciudadano confirmó haber recibido el mensaje `seq`.
    ClientAck,
    // Ubicación
    LocationReceived,
    LocationForwarded,
    // Videollamada
    VideoCallRequested,
    VideoCallUpdated,
    // Errores y anomalías
    ProtocolError,
    FieldsDropped,
    C5CallFailed,
    C5OrphanEvent,
    /// Exceso de mensajes o de sesiones nuevas (sin datos sensibles).
    RateLimited,
}

impl EventKind {
    /// Nombre estable (igual al de serde) para métricas, logs y SQL.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SessionKeyCreated => "session_key_created",
            Self::ConnectionOpened => "connection_opened",
            Self::ConnectionClosed => "connection_closed",
            Self::SessionStarted => "session_started",
            Self::SessionResumed => "session_resumed",
            Self::SessionResumeRejected => "session_resume_rejected",
            Self::SessionClosed => "session_closed",
            Self::ClientMetadata => "client_metadata",
            Self::EntryPoint => "entry_point",
            Self::ClientEvent => "client_event",
            Self::IncidentRequested => "incident_requested",
            Self::IncidentCreated => "incident_created",
            Self::IncidentStatusChanged => "incident_status_changed",
            Self::FirstOperatorResponse => "first_operator_response",
            Self::CitizenMessage => "citizen_message",
            Self::CitizenMessageForwarded => "citizen_message_forwarded",
            Self::CitizenMessageFailed => "citizen_message_failed",
            Self::OperatorMessage => "operator_message",
            Self::MessageStatusChanged => "message_status_changed",
            Self::ClientAck => "client_ack",
            Self::LocationReceived => "location_received",
            Self::LocationForwarded => "location_forwarded",
            Self::VideoCallRequested => "video_call_requested",
            Self::VideoCallUpdated => "video_call_updated",
            Self::ProtocolError => "protocol_error",
            Self::FieldsDropped => "fields_dropped",
            Self::C5CallFailed => "c5_call_failed",
            Self::C5OrphanEvent => "c5_orphan_event",
            Self::RateLimited => "rate_limited",
        }
    }
}

/// Contenido sensible de un evento. Nunca se registra en logs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SensitivePayload {
    Message {
        text: String,
    },
    Location {
        latitude: f64,
        longitude: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        accuracy_m: Option<f64>,
        source: String,
        captured_at: Timestamp,
    },
    Network {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ip: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        user_agent: Option<String>,
    },
    /// Datos que entrega el sistema de videollamadas (sala, token, grabación).
    VideoCall {
        data: serde_json::Value,
    },
}

/// Entrada del stream del registro: el evento (sin contenido sensible) y el
/// contenido sensible ya cifrado.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamRecord {
    pub event: RecordEvent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sealed: Option<SealedPayload>,
}

/// Contenido sensible cifrado con AES-256-GCM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedPayload {
    pub alg: String,
    pub key_ref: KeyRef,
    /// base64
    pub nonce: String,
    /// base64 (incluye la etiqueta GCM)
    pub ciphertext: String,
}

/// Dónde está la llave con que se cifró.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum KeyRef {
    /// La DEK de la sesión (registrada con `session_key_created`).
    Session { session_id: Uuid },
    /// Llave propia del evento, envuelta aquí mismo para la llave de
    /// investigación (eventos sin sesión o sin DEK disponible).
    Inline { wrapped: WrappedKeyData },
}

/// DEK envuelta con HPKE para la llave pública de investigación.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrappedKeyData {
    pub alg: String,
    pub recipient_key_id: String,
    /// base64
    pub encapped: String,
    /// base64
    pub ciphertext: String,
}

/// Datos asociados (AAD) del cifrado del contenido: ligan el texto cifrado a
/// su evento. Moverlo a otro registro hace fallar el descifrado.
pub fn sensitive_aad(event: &RecordEvent) -> Vec<u8> {
    let session = event
        .session_id
        .map(|s| s.to_string())
        .unwrap_or_else(|| "-".into());
    format!(
        "sb-rec-v1|{}|{}|{}",
        event.event_id,
        session,
        event.kind.as_str()
    )
    .into_bytes()
}

/// Contexto (AAD) de la envoltura de una DEK de sesión.
pub fn session_key_context(session_id: Uuid) -> Vec<u8> {
    format!("sb-dek-v1|session|{session_id}").into_bytes()
}

/// Contexto (AAD) de la envoltura de una DEK propia de un evento.
pub fn inline_key_context(event_id: Uuid) -> Vec<u8> {
    format!("sb-dek-v1|event|{event_id}").into_bytes()
}

/// Descifrado del contenido sensible (solo para la herramienta de
/// investigación, que tiene la llave privada).
pub mod open {
    use base64::{Engine, engine::general_purpose::STANDARD as B64};
    use crypto::{RecipientPrivateKey, Sealed, WrappedKey, unwrap_with_recipient};

    use super::{
        EventKind, KeyRef, RecordEvent, SealedPayload, SensitivePayload, WrappedKeyData,
        inline_key_context, sensitive_aad, session_key_context,
    };

    #[derive(Debug, thiserror::Error)]
    pub enum OpenError {
        #[error("falta la llave de la sesión")]
        MissingSessionKey,
        #[error("dato mal codificado: {0}")]
        Encoding(&'static str),
        #[error(transparent)]
        Crypto(#[from] crypto::CryptoError),
        #[error("contenido descifrado ilegible: {0}")]
        Json(#[from] serde_json::Error),
    }

    impl WrappedKeyData {
        /// Lee la DEK envuelta de un evento `session_key_created`.
        pub fn from_session_key_event(event: &RecordEvent) -> Option<Self> {
            if event.kind != EventKind::SessionKeyCreated {
                return None;
            }
            let attr = |k: &str| event.attributes.get(k)?.as_str().map(str::to_owned);
            Some(Self {
                alg: attr("alg")?,
                recipient_key_id: attr("recipient_key_id")?,
                encapped: attr("encapped")?,
                ciphertext: attr("wrapped_key")?,
            })
        }

        fn to_wrapped_key(&self) -> Result<WrappedKey, OpenError> {
            Ok(WrappedKey {
                alg: self.alg.clone(),
                recipient_key_id: self.recipient_key_id.clone(),
                encapped: B64
                    .decode(&self.encapped)
                    .map_err(|_| OpenError::Encoding("encapped"))?,
                ciphertext: B64
                    .decode(&self.ciphertext)
                    .map_err(|_| OpenError::Encoding("wrapped_key"))?,
            })
        }
    }

    /// Descifra el contenido de `event`. `session_key` es la DEK envuelta de
    /// su sesión (necesaria si `key_ref` es `session`).
    pub fn open_sealed(
        event: &RecordEvent,
        sealed: &SealedPayload,
        session_key: Option<&WrappedKeyData>,
        private: &RecipientPrivateKey,
    ) -> Result<SensitivePayload, OpenError> {
        let dek = match &sealed.key_ref {
            KeyRef::Session { session_id } => {
                let wrapped = session_key.ok_or(OpenError::MissingSessionKey)?;
                unwrap_with_recipient(
                    private,
                    &wrapped.to_wrapped_key()?,
                    &session_key_context(*session_id),
                )?
            }
            KeyRef::Inline { wrapped } => unwrap_with_recipient(
                private,
                &wrapped.to_wrapped_key()?,
                &inline_key_context(event.event_id),
            )?,
        };
        let nonce: [u8; 12] = B64
            .decode(&sealed.nonce)
            .ok()
            .and_then(|n| n.try_into().ok())
            .ok_or(OpenError::Encoding("nonce"))?;
        let ciphertext = B64
            .decode(&sealed.ciphertext)
            .map_err(|_| OpenError::Encoding("ciphertext"))?;
        let plain = crypto::open(&dek, &sensitive_aad(event), &Sealed { nonce, ciphertext })?;
        Ok(serde_json::from_slice(&plain)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_names_match_serde() {
        for kind in [
            EventKind::ConnectionOpened,
            EventKind::C5OrphanEvent,
            EventKind::ClientAck,
        ] {
            let serde_name = serde_json::to_value(kind).unwrap();
            assert_eq!(serde_name, kind.as_str());
        }
    }

    #[test]
    fn trace_id_is_extracted_from_traceparent() {
        let event = RecordEvent::new(EventKind::ClientAck, "bff/test").trace(Some(
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        ));
        assert_eq!(
            event.trace_id.as_deref(),
            Some("4bf92f3577b34da6a3ce929d0e0e4736")
        );
    }
}
