//! BFF de Sin Barreras: única puerta pública entre la PWA ciudadana y el C5.
//!
//! - WebSocket `/ws` con el protocolo de `crates/protocol`.
//! - Estado compartido entre réplicas en Valkey (el BFF no guarda estado
//!   propio: cualquier réplica puede atender cualquier sesión).
//! - Puerto `C5Gateway` con adaptadores intercambiables.
//! - Todo lo que pasa se registra mediante `Recorder` (registro completo).

pub mod c5;
pub mod config;
pub mod forwarder;
pub mod http;
pub mod hub;
pub mod inbound;
pub mod metrics;
pub mod qr;
pub mod ratelimit;
pub mod records;
pub mod session;
pub mod store;
pub mod token;
pub mod valkey;
pub mod ws;

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{HeaderValue, header},
    routing::{get, post},
};
use service_kit::health::{self, CheckStatus, ReadinessProbe};
use tokio::task::JoinHandle;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::{
    c5::C5Gateway,
    config::Config,
    forwarder::Forwarders,
    hub::Hub,
    qr::QrVerifier,
    records::Records,
    store::Store,
    valkey::{BUS_CHANNEL, Valkey},
};

pub struct AppState {
    pub config: Arc<Config>,
    pub store: Store,
    pub hub: Arc<Hub>,
    pub gateway: Arc<dyn C5Gateway>,
    pub records: Arc<Records>,
    pub qr: QrVerifier,
    pub forwarders: Forwarders,
    pub metrics: Arc<metrics::Metrics>,
}

impl AppState {
    /// Debe llamarse dentro de un runtime de tokio (arranca la tarea del registro).
    pub fn new(config: Config, valkey: Valkey, gateway: Arc<dyn C5Gateway>) -> Arc<Self> {
        let qr = QrVerifier::new(config.qr_keys.clone()).with_revoked(config.qr_revoked.clone());
        let producer = format!("bff/{}", config.replica_id);
        let records = Records::start(config.record_keys.clone(), valkey.client.clone(), producer);
        let state = Arc::new(Self {
            config: Arc::new(config),
            store: Store::new(valkey),
            hub: Arc::new(Hub::default()),
            gateway,
            records,
            qr,
            forwarders: Forwarders::default(),
            metrics: Arc::new(metrics::Metrics::default()),
        });
        state.metrics.attach(&state);
        state
    }
}

/// `/ready` del BFF depende SOLO de Valkey. Si el C5 está caído, el BFF debe
/// seguir aceptando y guardando los mensajes del ciudadano (se reenvían
/// cuando el C5 vuelva); sacar al BFF del balanceador sería peor.
#[derive(Clone)]
struct ValkeyProbe(Store);

impl ReadinessProbe for ValkeyProbe {
    async fn check(&self) -> BTreeMap<&'static str, CheckStatus> {
        let status =
            match tokio::time::timeout(Duration::from_secs(1), self.0.valkey().ping()).await {
                Ok(Ok(())) => CheckStatus::Ok,
                Ok(Err(_)) => CheckStatus::Failing {
                    reason: "error".into(),
                },
                Err(_) => CheckStatus::Failing {
                    reason: "timeout".into(),
                },
            };
        BTreeMap::from([("valkey", status)])
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    let health = health::router(ValkeyProbe(state.store.clone()), env!("CARGO_PKG_VERSION"));
    Router::new()
        .route("/ws", get(ws::handler))
        .route("/api/v1/hello", post(http::hello))
        .route("/api/v1/sync", post(http::sync))
        .layer(DefaultBodyLimit::max(http::MAX_BODY_BYTES))
        // Cabeceras de seguridad en todas las respuestas del BFF.
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .with_state(Arc::clone(&state))
        .merge(health)
        .merge(service_kit::metrics::router(Arc::clone(&state.metrics)))
}

/// Tareas de fondo de cada réplica: bus de mensajes, barrido de pendientes y
/// eventos del C5.
pub fn spawn_background(state: &Arc<AppState>) -> Vec<JoinHandle<()>> {
    vec![
        tokio::spawn(run_bus_listener(Arc::clone(state))),
        tokio::spawn(forwarder::run_sweeper(Arc::clone(state))),
        tokio::spawn(inbound::run(Arc::clone(state))),
    ]
}

async fn run_bus_listener(state: Arc<AppState>) {
    use fred::interfaces::EventInterface;
    let mut messages = state.store.valkey().subscriber.message_rx();
    loop {
        match messages.recv().await {
            Ok(message) => {
                if message.channel != BUS_CHANNEL {
                    continue;
                }
                let Some(raw) = valkey::value_to_string(&message.value) else {
                    continue;
                };
                if let Some((session_id, frame)) = Hub::parse_bus_message(&raw) {
                    state.hub.dispatch(session_id, frame);
                }
            }
            // Si se perdieron mensajes por retraso, cada conexión los recupera
            // por `seq` en su verificación periódica.
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(
                    lost = n,
                    "bus retrasado; las conexiones recuperarán por seq"
                );
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                tracing::error!("el canal del bus se cerró");
                return;
            }
        }
    }
}
