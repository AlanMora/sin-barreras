//! BFF de Sin Barreras: única puerta pública entre la PWA ciudadana y el C5.
//!
//! En la Fase 0 solo expone `/health` y `/ready`. El WebSocket, las sesiones
//! y el puerto `C5Gateway` llegan en la Fase 1.

pub mod config;

use axum::Router;
use service_kit::health::{self, NoDependencies};

/// Construye el router HTTP del servicio.
pub fn app() -> Router {
    health::router(NoDependencies, env!("CARGO_PKG_VERSION"))
}
