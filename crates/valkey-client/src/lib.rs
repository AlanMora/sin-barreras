//! Conexión a Valkey compartida por BFF y record-writer.
//!
//! - TLS con verificación contra nuestra CA (o las del sistema).
//! - Timeouts cortos: ninguna operación cuelga el flujo del ciudadano.
//! - Reconexión infinita con backoff exponencial (100 ms → 5 s).

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use fred::{
    prelude::*,
    rustls::{
        ClientConfig, RootCertStore,
        pki_types::{CertificateDer, pem::PemObject},
    },
    types::{
        Value,
        config::{ReconnectPolicy, TlsConnector},
    },
};
use service_kit::config::{ConfigError, env_or, secret};

#[derive(Debug, Clone)]
pub struct ValkeyConfig {
    /// `VALKEY_URL`: `valkeys://valkey:6379` (TLS) o `valkey://…` (solo pruebas).
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    /// `VALKEY_CA_FILE`: CA con la que se verifica el certificado del servidor.
    pub ca_file: Option<PathBuf>,
}

impl ValkeyConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            url: env_or("VALKEY_URL", "valkeys://valkey:6379".to_owned())?,
            username: std::env::var("VALKEY_USERNAME").ok(),
            password: secret("VALKEY_PASSWORD").ok(),
            ca_file: std::env::var("VALKEY_CA_FILE").ok().map(PathBuf::from),
        })
    }

    /// Para pruebas con un contenedor sin TLS ni autenticación.
    pub fn plain(url: &str) -> Self {
        Self {
            url: url.to_owned(),
            username: None,
            password: None,
            ca_file: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error("error de Valkey: {0}")]
    Client(#[from] fred::error::Error),
    #[error("no se pudo leer la CA de Valkey en {path}: {reason}")]
    Ca { path: String, reason: String },
}

/// Builder configurado (para crear clientes normales o suscriptores).
pub fn builder(cfg: &ValkeyConfig) -> Result<Builder, ConnectError> {
    let mut config = Config::from_url(&cfg.url)?;
    config.username = cfg.username.clone();
    config.password = cfg.password.clone();
    if config.tls.is_some() || cfg.url.starts_with("valkeys://") || cfg.url.starts_with("rediss://")
    {
        config.tls = Some(tls_connector(cfg.ca_file.as_deref())?.into());
    }
    let mut builder = Builder::from_config(config);
    builder
        .with_connection_config(|c| {
            c.connection_timeout = Duration::from_secs(5);
            c.internal_command_timeout = Duration::from_secs(5);
            c.max_command_attempts = 3;
        })
        .with_performance_config(|p| {
            p.default_command_timeout = Duration::from_secs(3);
        })
        .set_policy(ReconnectPolicy::new_exponential(0, 100, 5_000, 2));
    Ok(builder)
}

pub async fn connect(cfg: &ValkeyConfig) -> Result<Client, ConnectError> {
    let client = builder(cfg)?.build()?;
    client.init().await?;
    Ok(client)
}

/// Como [`connect`], pero reintenta indefinidamente con backoff. Para el
/// arranque de los servicios: si Valkey se está reiniciando (por ejemplo,
/// cargando el AOF), el servicio espera en lugar de morir y provocar
/// reinicios en cascada. Solo falla ante errores de configuración.
pub async fn connect_with_retry(cfg: &ValkeyConfig) -> Result<Client, ConnectError> {
    let mut backoff =
        service_kit::backoff::Backoff::new(Duration::from_millis(200), Duration::from_secs(5));
    loop {
        match connect(cfg).await {
            Ok(client) => return Ok(client),
            Err(ConnectError::Ca { path, reason }) => {
                return Err(ConnectError::Ca { path, reason });
            }
            Err(error) => {
                tracing::warn!(%error, "Valkey no disponible; reintentando");
                tokio::time::sleep(backoff.next_delay()).await;
            }
        }
    }
}

fn tls_connector(ca_file: Option<&Path>) -> Result<TlsConnector, ConnectError> {
    let mut roots = RootCertStore::empty();
    match ca_file {
        Some(path) => {
            let ca_error = |reason: String| ConnectError::Ca {
                path: path.display().to_string(),
                reason,
            };
            for cert in CertificateDer::pem_file_iter(path).map_err(|e| ca_error(e.to_string()))? {
                let cert = cert.map_err(|e| ca_error(e.to_string()))?;
                roots.add(cert).map_err(|e| ca_error(e.to_string()))?;
            }
        }
        None => {
            for cert in fred::rustls_native_certs::load_native_certs().certs {
                let _ = roots.add(cert);
            }
        }
    }
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(TlsConnector::from(config))
}

/// Convierte un `Value` de Valkey a texto (respuestas de scripts).
pub fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.to_string()),
        Value::Bytes(b) => String::from_utf8(b.to_vec()).ok(),
        Value::Integer(i) => Some(i.to_string()),
        _ => None,
    }
}
