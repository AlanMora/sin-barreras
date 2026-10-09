//! Inicialización de logs estructurados.
//!
//! POLÍTICA: los logs de aplicación NUNCA deben contener el contenido de
//! mensajes, ubicaciones ni IPs. Esos datos solo viven cifrados en Postgres.
//! Registra identificadores (sesión, incidente, evento), nunca cargas útiles.

use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[derive(Debug, thiserror::Error)]
pub enum TelemetryError {
    #[error("no se pudo inicializar el subscriber de tracing: {0}")]
    Init(String),
}

/// Formato de salida de los logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// JSON de una línea por evento (producción).
    Json,
    /// Texto legible (desarrollo local).
    Pretty,
}

impl LogFormat {
    /// Lee `LOG_FORMAT` (`json` | `pretty`); por defecto `json`.
    pub fn from_env() -> Self {
        match std::env::var("LOG_FORMAT").as_deref() {
            Ok("pretty") => Self::Pretty,
            _ => Self::Json,
        }
    }
}

/// Inicializa el subscriber global. El nivel se controla con `RUST_LOG`
/// (por defecto `info`).
pub fn init(service_name: &'static str) -> Result<(), TelemetryError> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let registry = tracing_subscriber::registry().with(filter);

    let result = match LogFormat::from_env() {
        LogFormat::Json => registry
            .with(
                fmt::layer()
                    .json()
                    .flatten_event(true)
                    .with_current_span(false),
            )
            .try_init(),
        LogFormat::Pretty => registry.with(fmt::layer()).try_init(),
    };
    result.map_err(|e| TelemetryError::Init(e.to_string()))?;

    tracing::info!(
        service = service_name,
        version = env!("CARGO_PKG_VERSION"),
        "telemetría lista"
    );
    Ok(())
}
