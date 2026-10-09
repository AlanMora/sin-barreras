//! Configuración del BFF, leída de variables de entorno y Docker secrets.

use std::{collections::HashMap, net::SocketAddr, path::PathBuf, time::Duration};

use service_kit::config::{ConfigError, env_or, secret};

#[derive(Debug, Clone)]
pub struct Config {
    /// `BFF_LISTEN_ADDR`
    pub listen_addr: SocketAddr,
    /// `BFF_REPLICA_ID` (por defecto, el hostname del contenedor).
    pub replica_id: String,
    pub valkey: ValkeyConfig,
    pub c5: C5Config,
    pub sessions: SessionConfig,
    /// Llaves HMAC de los QR por id (`QR_KEYS`/`QR_KEYS_FILE`: `1:<base64>,2:<base64>`).
    pub qr_keys: HashMap<String, Vec<u8>>,
    /// Orígenes permitidos para abrir el WebSocket (`ALLOWED_ORIGINS`, separados
    /// por coma; `*` solo en desarrollo).
    pub allowed_origins: Vec<String>,
    /// Vigencia del lease de reenvío al C5 (`FORWARD_LEASE_TTL_MS`). Si una
    /// réplica muere, otra retoma sus pendientes tras este tiempo.
    pub forward_lease_ttl: Duration,
}

#[derive(Debug, Clone)]
pub struct ValkeyConfig {
    /// `VALKEY_URL`: `valkeys://valkey:6379` (TLS) o `valkey://…` (solo pruebas).
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    /// `VALKEY_CA_FILE`: CA con la que se verifica el certificado del servidor.
    pub ca_file: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct C5Config {
    /// `C5_MOCK_URL`: base del simulador (`http://c5-mock:8090`).
    pub mock_url: String,
    pub mock_api_key: String,
    pub request_timeout: Duration,
}

#[derive(Debug, Clone, Copy)]
pub struct SessionConfig {
    /// TTL de una sesión inactiva sin incidente abierto.
    pub idle_ttl: Duration,
    /// TTL de una sesión inactiva con un incidente abierto (una persona puede
    /// bloquear el teléfono durante horas y debe poder volver a su conversación).
    pub incident_ttl: Duration,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let replica_id = std::env::var("BFF_REPLICA_ID")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| format!("bff-{}", uuid::Uuid::new_v4().simple()));

        Ok(Self {
            listen_addr: env_or("BFF_LISTEN_ADDR", SocketAddr::from(([0, 0, 0, 0], 8080)))?,
            replica_id,
            valkey: ValkeyConfig {
                url: env_or("VALKEY_URL", "valkeys://valkey:6379".to_owned())?,
                username: std::env::var("VALKEY_USERNAME").ok(),
                password: secret("VALKEY_PASSWORD").ok(),
                ca_file: std::env::var("VALKEY_CA_FILE").ok().map(PathBuf::from),
            },
            c5: C5Config {
                mock_url: env_or("C5_MOCK_URL", "http://c5-mock:8090".to_owned())?,
                mock_api_key: secret("C5_MOCK_API_KEY").unwrap_or_default(),
                request_timeout: Duration::from_millis(env_or("C5_TIMEOUT_MS", 10_000)?),
            },
            sessions: SessionConfig {
                idle_ttl: Duration::from_secs(env_or("SESSION_IDLE_TTL_SECS", 30 * 60)?),
                incident_ttl: Duration::from_secs(env_or(
                    "SESSION_INCIDENT_TTL_SECS",
                    12 * 60 * 60,
                )?),
            },
            qr_keys: parse_qr_keys(&secret("QR_KEYS").unwrap_or_default())?,
            allowed_origins: env_or("ALLOWED_ORIGINS", String::new())?
                .split(',')
                .map(str::trim)
                .filter(|o| !o.is_empty())
                .map(str::to_owned)
                .collect(),
            forward_lease_ttl: Duration::from_millis(env_or("FORWARD_LEASE_TTL_MS", 15_000)?),
        })
    }
}

/// `1:<base64>,2:<base64>` → {"1": bytes, "2": bytes}
pub fn parse_qr_keys(raw: &str) -> Result<HashMap<String, Vec<u8>>, ConfigError> {
    use base64::Engine;
    let invalid = |reason: &str| ConfigError::Invalid {
        key: "QR_KEYS".into(),
        reason: reason.into(),
    };
    let mut keys = HashMap::new();
    for entry in raw.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let (id, encoded) = entry
            .split_once(':')
            .ok_or_else(|| invalid("formato id:base64"))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(encoded))
            .map_err(|_| invalid("base64 inválido"))?;
        if bytes.len() < 32 {
            return Err(invalid("cada llave debe tener al menos 32 bytes"));
        }
        keys.insert(id.to_owned(), bytes);
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_qr_keys() {
        let key = "A".repeat(44); // 33 bytes en base64
        let keys = parse_qr_keys(&format!("1:{key}, 2:{key}")).unwrap();
        assert_eq!(keys.len(), 2);
        assert!(parse_qr_keys("1:abc").is_err());
        assert!(parse_qr_keys("sin-separador").is_err());
        assert!(parse_qr_keys("").unwrap().is_empty());
    }
}
