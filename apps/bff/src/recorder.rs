//! Punto único por donde el BFF manda eventos al registro completo.
//!
//! REGLA DE ORO 1: `record` nunca bloquea ni falla hacia quien lo llama. La
//! implementación real (Valkey Streams + cifrado) vive en `record_stream`.

use std::sync::Mutex;

use events::RecordEvent;

pub trait Recorder: Send + Sync + 'static {
    /// Encola un evento para el registro. No bloquea.
    fn record(&self, event: RecordEvent);
}

/// Solo deja constancia en logs del tipo de evento y sus ids (nunca del
/// contenido). Útil en pruebas unitarias.
#[derive(Debug, Default)]
pub struct LogRecorder;

impl Recorder for LogRecorder {
    fn record(&self, event: RecordEvent) {
        tracing::debug!(
            kind = event.kind.as_str(),
            event_id = %event.event_id,
            session_id = ?event.session_id,
            "evento de registro"
        );
    }
}

/// Guarda los eventos en memoria (pruebas).
#[derive(Debug, Default)]
pub struct MemoryRecorder {
    events: Mutex<Vec<RecordEvent>>,
}

impl MemoryRecorder {
    pub fn events(&self) -> Vec<RecordEvent> {
        self.events.lock().map(|e| e.clone()).unwrap_or_default()
    }
}

impl Recorder for MemoryRecorder {
    fn record(&self, event: RecordEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
    }
}
