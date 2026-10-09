//! Simulador del C5 para desarrollo y pruebas.
//!
//! IMPORTANTE: la API que expone este simulador es inventada por nosotros. NO
//! refleja la API real del C5, que aún no está documentada. El comportamiento
//! de operador (respuestas, cambios de estado, latencia, desconexiones) llega
//! en la Fase 1.

pub mod config;

use axum::Router;
use service_kit::health::{self, NoDependencies};

/// Construye el router HTTP del servicio.
pub fn app() -> Router {
    health::router(NoDependencies, env!("CARGO_PKG_VERSION"))
}
