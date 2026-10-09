//! Configuración de record-writer.

use std::{net::SocketAddr, time::Duration};

use registry::db::DbConfig;
use service_kit::config::{ConfigError, env_or};
use valkey_client::ValkeyConfig;

#[derive(Debug, Clone)]
pub struct Config {
    /// `RECORD_WRITER_LISTEN_ADDR`
    pub listen_addr: SocketAddr,
    /// Nombre del consumidor en el consumer group (`RECORD_WRITER_CONSUMER`).
    /// Debe ser estable por réplica (en Swarm: `record-writer-{{.Task.Slot}}`)
    /// para que, al reiniciar, retome sus propios pendientes.
    pub consumer: String,
    pub valkey: ValkeyConfig,
    pub db: DbConfig,
    pub tuning: Tuning,
}

#[derive(Debug, Clone, Copy)]
pub struct Tuning {
    /// Eventos por lote (`RECORD_WRITER_BATCH`).
    pub batch_size: u64,
    /// Espera máxima por eventos nuevos (`RECORD_WRITER_BLOCK_MS`).
    pub block: Duration,
    /// Inactividad tras la cual se reasignan pendientes de un consumidor
    /// muerto (`RECORD_WRITER_CLAIM_IDLE_MS`).
    pub claim_idle: Duration,
    /// Entregas tras las cuales un evento va a la cola de errores
    /// (`RECORD_WRITER_MAX_DELIVERIES`).
    pub max_deliveries: u64,
    /// Cada cuánto se crea un checkpoint (`RECORD_WRITER_CHECKPOINT_SECS`).
    pub checkpoint_every: Duration,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            batch_size: 100,
            block: Duration::from_millis(2_000),
            claim_idle: Duration::from_secs(30),
            max_deliveries: 5,
            checkpoint_every: Duration::from_secs(300),
        }
    }
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let d = Tuning::default();
        let consumer = std::env::var("RECORD_WRITER_CONSUMER")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "record-writer".to_owned());
        Ok(Self {
            listen_addr: env_or(
                "RECORD_WRITER_LISTEN_ADDR",
                SocketAddr::from(([0, 0, 0, 0], 8081)),
            )?,
            consumer,
            valkey: ValkeyConfig::from_env()?,
            db: DbConfig::from_env("record-writer")?,
            tuning: Tuning {
                batch_size: env_or("RECORD_WRITER_BATCH", d.batch_size)?,
                block: Duration::from_millis(env_or("RECORD_WRITER_BLOCK_MS", 2_000)?),
                claim_idle: Duration::from_millis(env_or("RECORD_WRITER_CLAIM_IDLE_MS", 30_000)?),
                max_deliveries: env_or("RECORD_WRITER_MAX_DELIVERIES", d.max_deliveries)?,
                checkpoint_every: Duration::from_secs(env_or(
                    "RECORD_WRITER_CHECKPOINT_SECS",
                    300,
                )?),
            },
        })
    }
}
