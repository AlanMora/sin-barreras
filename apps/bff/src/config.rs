//! Configuración de bff, leída de variables de entorno.

use std::net::SocketAddr;

use service_kit::config::{ConfigError, env_or};

#[derive(Debug, Clone)]
pub struct Config {
    /// Dirección donde escucha el servidor HTTP (`BFF_LISTEN_ADDR`).
    pub listen_addr: SocketAddr,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            listen_addr: env_or("BFF_LISTEN_ADDR", SocketAddr::from(([0, 0, 0, 0], 8080)))?,
        })
    }
}
