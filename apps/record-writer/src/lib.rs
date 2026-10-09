//! record-writer: guarda el registro completo e íntegro del lado ciudadano.
//!
//! Consume el stream de eventos con consumer groups y confirma (XACK) solo
//! después de escribir en Postgres. La lógica llega en la Fase 2.

pub mod config;

use axum::Router;
use service_kit::health::{self, NoDependencies};

/// Construye el router HTTP del servicio.
pub fn app() -> Router {
    health::router(NoDependencies, env!("CARGO_PKG_VERSION"))
}
