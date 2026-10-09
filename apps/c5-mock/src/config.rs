//! Configuración del simulador del C5.

use std::net::SocketAddr;

use service_kit::config::{ConfigError, env_or, secret};

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_addr: SocketAddr,
    /// Llave que deben enviar los clientes en `x-api-key` (vacía = sin auth).
    pub api_key: String,
    pub chaos: Chaos,
    /// Si el operador simulado responde solo.
    pub auto_operator: bool,
}

/// Fallas inyectadas para probar la resiliencia del BFF.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Chaos {
    pub latency_min_ms: u64,
    pub latency_max_ms: u64,
    /// Probabilidad (0.0–1.0) de responder 503 en las operaciones.
    pub failure_rate: f64,
    /// Cada cuántos segundos se cortan las conexiones del feed (0 = nunca).
    pub feed_drop_every_secs: u64,
}

impl Default for Chaos {
    fn default() -> Self {
        Self {
            latency_min_ms: 50,
            latency_max_ms: 300,
            failure_rate: 0.0,
            feed_drop_every_secs: 0,
        }
    }
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let defaults = Chaos::default();
        Ok(Self {
            listen_addr: env_or(
                "C5_MOCK_LISTEN_ADDR",
                SocketAddr::from(([0, 0, 0, 0], 8090)),
            )?,
            api_key: secret("C5_MOCK_API_KEY").unwrap_or_default(),
            chaos: Chaos {
                latency_min_ms: env_or("C5_MOCK_LATENCY_MIN_MS", defaults.latency_min_ms)?,
                latency_max_ms: env_or("C5_MOCK_LATENCY_MAX_MS", defaults.latency_max_ms)?,
                failure_rate: env_or("C5_MOCK_FAILURE_RATE", defaults.failure_rate)?,
                feed_drop_every_secs: env_or(
                    "C5_MOCK_FEED_DROP_EVERY_SECS",
                    defaults.feed_drop_every_secs,
                )?,
            },
            auto_operator: env_or("C5_MOCK_AUTO_OPERATOR", true)?,
        })
    }
}
