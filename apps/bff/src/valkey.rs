//! Conexión a Valkey y operaciones atómicas (scripts Lua) del BFF.
//!
//! Todas las operaciones que necesitan garantías (aceptar un comando sin
//! duplicarlo, asignar `seq`, reenviar al C5 en orden) son scripts Lua: Valkey
//! los ejecuta de forma atómica, así dos réplicas nunca ven un estado a medias.

use std::sync::Arc;

use fred::{
    clients::SubscriberClient,
    prelude::*,
    types::{Value, scripts::Script},
};
use uuid::Uuid;
pub use valkey_client::{ValkeyConfig, value_to_string};

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
    #[error(transparent)]
    Connect(#[from] valkey_client::ConnectError),
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
        let builder = valkey_client::builder(cfg)?;
        // La conexión inicial reintenta (Valkey puede estar reiniciando).
        let client = valkey_client::connect_with_retry(cfg).await?;
        let subscriber = builder.build_subscriber_client()?;
        let mut backoff = service_kit::backoff::Backoff::new(
            std::time::Duration::from_millis(200),
            std::time::Duration::from_secs(5),
        );
        while let Err(error) = subscriber.init().await {
            tracing::warn!(%error, "suscriptor de Valkey no disponible; reintentando");
            tokio::time::sleep(backoff.next_delay()).await;
        }
        // Re-suscribe automáticamente tras una reconexión.
        let _resubscribe = subscriber.manage_subscriptions();
        subscriber.subscribe(BUS_CHANNEL).await?;
        Ok(Self {
            client,
            subscriber,
            scripts: Arc::new(Scripts::new()),
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
