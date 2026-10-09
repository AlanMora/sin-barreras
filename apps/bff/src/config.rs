//! Configuración del BFF, leída de variables de entorno y Docker secrets.

use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    time::Duration,
};

use crypto::{OperationalKey, RecipientPublicKey};
use service_kit::config::{ConfigError, env_or, secret};

use crate::{ratelimit::Limits, records::RecordKeys};

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
    /// Llaves del registro: `RECORD_PUBLIC_KEY(_FILE)` (`id:base64`, pública de
    /// investigación) y `RECORD_OPERATIONAL_KEYS(_FILE)` (`id:base64,…`, la
    /// primera es la activa).
    pub record_keys: RecordKeys,
    /// Umbrales de rate limiting (`RATE_*`, ver ADR-0020).
    pub limits: Limits,
    /// Puntos con QR revocado (`QR_REVOKED_POINTS`, separados por coma): sus
    /// QR quedan "no verificados" aunque la firma sea válida.
    pub qr_revoked: HashSet<String>,
}

pub use valkey_client::ValkeyConfig;

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
            valkey: ValkeyConfig::from_env()?,
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
            record_keys: record_keys_from_env()?,
            limits: limits_from_env()?,
            qr_revoked: env_or("QR_REVOKED_POINTS", String::new())?
                .split(',')
                .map(|p| p.trim().to_uppercase())
                .filter(|p| !p.is_empty())
                .collect(),
        })
    }
}

fn limits_from_env() -> Result<Limits, ConfigError> {
    let d = Limits::default();
    Ok(Limits {
        chat_per_min: env_or("RATE_CHAT_PER_MIN", d.chat_per_min)?,
        free_messages: env_or("RATE_FREE_MESSAGES", d.free_messages)?,
        location_per_min: env_or("RATE_LOCATION_PER_MIN", d.location_per_min)?,
        client_events_per_min: env_or("RATE_CLIENT_EVENTS_PER_MIN", d.client_events_per_min)?,
        call_requests_per_min: env_or("RATE_CALL_REQUESTS_PER_MIN", d.call_requests_per_min)?,
        ip_observe_per_min: env_or("RATE_IP_OBSERVE_PER_MIN", d.ip_observe_per_min)?,
        ip_hard_per_min: env_or("RATE_IP_HARD_PER_MIN", d.ip_hard_per_min)?,
    })
}

fn record_keys_from_env() -> Result<RecordKeys, ConfigError> {
    let invalid = |key: &str, e: crypto::CryptoError| ConfigError::Invalid {
        key: key.into(),
        reason: e.to_string(),
    };
    let recipient = RecipientPublicKey::parse(&secret("RECORD_PUBLIC_KEY")?)
        .map_err(|e| invalid("RECORD_PUBLIC_KEY", e))?;
    let operational = secret("RECORD_OPERATIONAL_KEYS")?
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .map(OperationalKey::parse)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| invalid("RECORD_OPERATIONAL_KEYS", e))?;
    if operational.is_empty() {
        return Err(ConfigError::Missing("RECORD_OPERATIONAL_KEYS".into()));
    }
    Ok(RecordKeys {
        recipient,
        operational,
    })
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
