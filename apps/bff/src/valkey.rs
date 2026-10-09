//! Conexión a Valkey y operaciones atómicas (scripts Lua) del BFF.
//!
//! Todas las operaciones que necesitan garantías (aceptar un comando sin
//! duplicarlo, asignar `seq`, reenviar al C5 en orden) son scripts Lua: Valkey
//! los ejecuta de forma atómica, así dos réplicas nunca ven un estado a medias.

use std::{path::Path, sync::Arc, time::Duration};

use fred::{
    clients::SubscriberClient,
    prelude::*,
    rustls::{
        ClientConfig, RootCertStore,
        pki_types::{CertificateDer, pem::PemObject},
    },
    types::{
        Value,
        config::{ReconnectPolicy, TlsConnector},
        scripts::Script,
    },
};
use uuid::Uuid;

use crate::config::ValkeyConfig;

const COMMON: &str = include_str!("lua/common.lua");

/// Canal de pub/sub donde se publican los mensajes secuenciados.
pub const BUS_CHANNEL: &str = "sb:bus";
/// Conjunto ordenado de sesiones con comandos pendientes de reenviar al C5.
pub const SWEEP_KEY: &str = "sb:sweep";
/// Marcador de "sin deduplicación" para el script `emit`.
pub const NO_DEDUPE: &str = "sb:nodedupe";

#[derive(Debug, thiserror::Error)]
pub enum ValkeyError {
    #[error("error de Valkey: {0}")]
    Client(#[from] fred::error::Error),
    #[error("no se pudo leer la CA de Valkey en {path}: {reason}")]
    Ca { path: String, reason: String },
    #[error("respuesta inesperada de Valkey: {0}")]
    Unexpected(String),
}

/// Llaves de una sesión. Todas comparten el hash tag `{sid}`.
#[derive(Debug, Clone)]
pub struct SessionKeys {
    pub sess: String,
    pub seq: String,
    pub out: String,
    pub inbox: String,
    pub cmids: String,
    pub fwd: String,
}

impl SessionKeys {
    pub fn new(session_id: Uuid) -> Self {
        let base = format!("sb:s:{{{session_id}}}");
        Self {
            seq: format!("{base}:seq"),
            out: format!("{base}:out"),
            inbox: format!("{base}:in"),
            cmids: format!("{base}:cmids"),
            fwd: format!("{base}:fwd"),
            sess: base,
        }
    }

    /// Las cinco llaves de datos (sin el lease), en el orden que esperan los scripts.
    pub fn data(&self) -> Vec<String> {
        vec![
            self.sess.clone(),
            self.seq.clone(),
            self.out.clone(),
            self.inbox.clone(),
            self.cmids.clone(),
        ]
    }
}

pub fn c5_incident_key(c5_incident_id: &str) -> String {
    format!("sb:c5i:{c5_incident_id}")
}

pub fn c5_event_key(c5_event_id: &str) -> String {
    format!("sb:c5e:{c5_event_id}")
}

/// Scripts precompilados (se cargan por SHA y se recargan si Valkey reinicia).
#[derive(Clone)]
pub struct Scripts {
    pub session_create: Script,
    pub session_resume: Script,
    pub touch: Script,
    pub accept: Script,
    pub emit: Script,
    pub lease_acquire: Script,
    pub lease_release: Script,
    pub mark_forwarded: Script,
}

impl Scripts {
    fn new() -> Self {
        let with_common = |body: &str| Script::from_lua(format!("{COMMON}\n{body}"));
        Self {
            session_create: with_common(include_str!("lua/session_create.lua")),
            session_resume: with_common(include_str!("lua/session_resume.lua")),
            touch: with_common(include_str!("lua/touch.lua")),
            accept: with_common(include_str!("lua/accept.lua")),
            emit: with_common(include_str!("lua/emit.lua")),
            lease_acquire: Script::from_lua(include_str!("lua/lease_acquire.lua")),
            lease_release: Script::from_lua(include_str!("lua/lease_release.lua")),
            mark_forwarded: Script::from_lua(include_str!("lua/mark_forwarded.lua")),
        }
    }
}

/// Cliente de Valkey del BFF: comandos + suscriptor del bus.
#[derive(Clone)]
pub struct Valkey {
    pub client: Client,
    pub subscriber: SubscriberClient,
    pub scripts: Arc<Scripts>,
}

impl Valkey {
    pub async fn connect(cfg: &ValkeyConfig) -> Result<Self, ValkeyError> {
        let mut config = Config::from_url(&cfg.url)?;
        config.username = cfg.username.clone();
        config.password = cfg.password.clone();
        if config.tls.is_some()
            || cfg.url.starts_with("valkeys://")
            || cfg.url.starts_with("rediss://")
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
                // Ninguna operación del BFF debe colgar el flujo del ciudadano.
                p.default_command_timeout = Duration::from_secs(3);
            })
            // Reintentos infinitos con backoff exponencial (100 ms → 5 s).
            .set_policy(ReconnectPolicy::new_exponential(0, 100, 5_000, 2));

        let client = builder.build()?;
        let subscriber = builder.build_subscriber_client()?;
        client.init().await?;
        subscriber.init().await?;
        // Re-suscribe automáticamente tras una reconexión.
        let _resubscribe = subscriber.manage_subscriptions();
        subscriber.subscribe(BUS_CHANNEL).await?;

        let scripts = Arc::new(Scripts::new());
        Ok(Self {
            client,
            subscriber,
            scripts,
        })
    }

    pub async fn ping(&self) -> Result<(), ValkeyError> {
        let _: String = self.client.ping(None).await?;
        Ok(())
    }

    /// Ejecuta un script (EVALSHA con recarga automática si Valkey lo olvidó).
    pub async fn run<R: FromValue>(
        &self,
        script: &Script,
        keys: Vec<String>,
        args: Vec<Value>,
    ) -> Result<R, ValkeyError> {
        Ok(script.evalsha_with_reload(&self.client, keys, args).await?)
    }
}

fn tls_connector(ca_file: Option<&Path>) -> Result<TlsConnector, ValkeyError> {
    let mut roots = RootCertStore::empty();
    match ca_file {
        Some(path) => {
            let ca_error = |reason: String| ValkeyError::Ca {
                path: path.display().to_string(),
                reason,
            };
            for cert in CertificateDer::pem_file_iter(path).map_err(|e| ca_error(e.to_string()))? {
                let cert = cert.map_err(|e| ca_error(e.to_string()))?;
                roots.add(cert).map_err(|e| ca_error(e.to_string()))?;
            }
        }
        None => {
            let native = fred::rustls_native_certs::load_native_certs();
            for cert in native.certs {
                let _ = roots.add(cert);
            }
        }
    }
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(TlsConnector::from(config))
}

/// Convierte un `Value` de Valkey a texto (para respuestas de scripts).
pub fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.to_string()),
        Value::Bytes(b) => String::from_utf8(b.to_vec()).ok(),
        Value::Integer(i) => Some(i.to_string()),
        _ => None,
    }
}
