//! Métricas técnicas del BFF (Prometheus). Ver `infra/observability`.

use std::sync::{Arc, atomic::AtomicU64};

use prometheus_client::{
    encoding::EncodeLabelSet,
    metrics::{counter::Counter, family::Family, gauge::Gauge, histogram::Histogram},
    registry::Registry,
};
use service_kit::metrics::MetricsSource;

use crate::AppState;

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct KindLabel {
    pub kind: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct TransportLabel {
    pub transport: String,
}

pub struct Metrics {
    registry: Registry,
    /// Comandos aceptados (durables) por tipo.
    pub commands_accepted: Family<KindLabel, Counter>,
    /// Sesiones nuevas y reanudadas, por transporte.
    pub sessions_started: Family<TransportLabel, Counter>,
    pub sessions_resumed: Family<TransportLabel, Counter>,
    /// Latencia desde que el ciudadano envió un mensaje hasta que el C5 lo recibió.
    pub delivery_latency_seconds: Histogram,
    /// Fallas al llamar al C5, por operación.
    pub c5_errors: Family<KindLabel, Counter>,
    /// Comandos o sesiones limitados, por ámbito.
    pub rate_limited: Family<KindLabel, Counter>,
    // Calculadas al consultar:
    pub ws_connections: Gauge,
    pub forward_backlog_sessions: Gauge,
    pub records_dropped: Counter<u64, AtomicU64>,
    state: std::sync::OnceLock<std::sync::Weak<AppState>>,
}

impl Default for Metrics {
    fn default() -> Self {
        let mut registry = Registry::with_prefix("sb_bff");
        let commands_accepted = Family::<KindLabel, Counter>::default();
        let sessions_started = Family::<TransportLabel, Counter>::default();
        let sessions_resumed = Family::<TransportLabel, Counter>::default();
        let delivery_latency_seconds =
            Histogram::new([0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 300.0]);
        let c5_errors = Family::<KindLabel, Counter>::default();
        let rate_limited = Family::<KindLabel, Counter>::default();
        let ws_connections = Gauge::default();
        let forward_backlog_sessions = Gauge::default();
        let records_dropped = Counter::<u64, AtomicU64>::default();
        registry.register(
            "commands_accepted",
            "Comandos del ciudadano aceptados de forma durable",
            commands_accepted.clone(),
        );
        registry.register(
            "sessions_started",
            "Sesiones nuevas",
            sessions_started.clone(),
        );
        registry.register(
            "sessions_resumed",
            "Reconexiones con sesión reanudada",
            sessions_resumed.clone(),
        );
        registry.register(
            "delivery_latency_seconds",
            "Tiempo desde que el ciudadano envía un mensaje hasta que el C5 lo recibe",
            delivery_latency_seconds.clone(),
        );
        registry.register(
            "c5_errors",
            "Fallas al llamar al C5 por operación",
            c5_errors.clone(),
        );
        registry.register(
            "rate_limited",
            "Comandos o sesiones limitados por ámbito",
            rate_limited.clone(),
        );
        registry.register(
            "ws_connections",
            "Conexiones WebSocket activas en esta réplica",
            ws_connections.clone(),
        );
        registry.register(
            "forward_backlog_sessions",
            "Sesiones con comandos aún sin entregar al C5 (todas las réplicas)",
            forward_backlog_sessions.clone(),
        );
        registry.register(
            "records_dropped",
            "Eventos técnicos del registro descartados por canal lleno (debe ser 0)",
            records_dropped.clone(),
        );
        Self {
            registry,
            commands_accepted,
            sessions_started,
            sessions_resumed,
            delivery_latency_seconds,
            c5_errors,
            rate_limited,
            ws_connections,
            forward_backlog_sessions,
            records_dropped,
            state: std::sync::OnceLock::new(),
        }
    }
}

impl Metrics {
    pub fn attach(&self, state: &Arc<AppState>) {
        let _ = self.state.set(Arc::downgrade(state));
    }

    pub fn kind(family: &Family<KindLabel, Counter>, kind: &str) {
        family
            .get_or_create(&KindLabel {
                kind: kind.to_owned(),
            })
            .inc();
    }

    pub fn transport(family: &Family<TransportLabel, Counter>, transport: &str) {
        family
            .get_or_create(&TransportLabel {
                transport: transport.to_owned(),
            })
            .inc();
    }
}

impl MetricsSource for Metrics {
    async fn refresh(&self) {
        let Some(state) = self.state.get().and_then(std::sync::Weak::upgrade) else {
            return;
        };
        self.ws_connections
            .set(i64::try_from(state.hub.active_connections()).unwrap_or(i64::MAX));
        if let Ok(backlog) = state.store.sweep_backlog().await {
            self.forward_backlog_sessions
                .set(i64::try_from(backlog).unwrap_or(i64::MAX));
        }
        let dropped = state.records.dropped();
        let current = self.records_dropped.get();
        if dropped > current {
            self.records_dropped.inc_by(dropped - current);
        }
    }

    fn registry(&self) -> &Registry {
        &self.registry
    }
}
