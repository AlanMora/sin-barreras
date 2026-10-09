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

use protocol::Timestamp;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Versión del esquema del sobre. Se incrementa ante cambios incompatibles.
pub const SCHEMA_VERSION: u16 = 1;

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
}

impl EventKind {
    /// Nombre estable (igual al de serde) para métricas, logs y SQL.
    pub fn as_str(&self) -> &'static str {
        match self {
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
