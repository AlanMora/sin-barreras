//! Métricas de record-writer (Prometheus): avance, cola de errores y retraso.

use std::sync::Arc;

use prometheus_client::{metrics::gauge::Gauge, registry::Registry};
use protocol::Timestamp;
use service_kit::metrics::MetricsSource;

use crate::consumer::Consumer;

pub struct WriterMetrics {
    consumer: Arc<Consumer>,
    registry: Registry,
    backlog: Gauge,
    dead_letters: Gauge,
    delay_seconds: Gauge,
}

impl WriterMetrics {
    pub fn new(consumer: Arc<Consumer>) -> Self {
        let mut registry = Registry::with_prefix("sb_record_writer");
        let stats = &consumer.stats;
        registry.register(
            "inserted",
            "Registros guardados en Postgres",
            stats.inserted.clone(),
        );
        registry.register(
            "duplicates",
            "Eventos ya guardados (reintentos idempotentes)",
            stats.duplicates.clone(),
        );
        registry.register(
            "dead_lettered",
            "Eventos enviados a la cola de errores",
            stats.dead_lettered.clone(),
        );
        registry.register(
            "claimed",
            "Pendientes reclamados a consumidores inactivos",
            stats.claimed.clone(),
        );
        registry.register(
            "transient_failures",
            "Fallas temporales de Postgres (los eventos esperan en el stream)",
            stats.transient_failures.clone(),
        );
        let backlog = Gauge::default();
        let dead_letters = Gauge::default();
        let delay_seconds = Gauge::default();
        registry.register(
            "stream_backlog",
            "Eventos aún no guardados (pendientes + sin leer)",
            backlog.clone(),
        );
        registry.register(
            "dead_letters",
            "Eventos en la cola de errores (debe ser 0)",
            dead_letters.clone(),
        );
        registry.register(
            "delay_seconds",
            "Antigüedad del evento más viejo aún no guardado en Postgres",
            delay_seconds.clone(),
        );
        Self {
            consumer,
            registry,
            backlog,
            dead_letters,
            delay_seconds,
        }
    }
}

impl MetricsSource for WriterMetrics {
    async fn refresh(&self) {
        if let Ok((lag, dlq)) = self.consumer.backlog().await {
            self.backlog.set(i64::try_from(lag).unwrap_or(i64::MAX));
            self.dead_letters
                .set(i64::try_from(dlq).unwrap_or(i64::MAX));
        }
        if let Ok(oldest) = self.consumer.oldest_unsaved_ms().await {
            let now = Timestamp::now().unix_millis().unsigned_abs();
            let delay = oldest.map_or(0, |ms| now.saturating_sub(ms) / 1000);
            self.delay_seconds
                .set(i64::try_from(delay).unwrap_or(i64::MAX));
        }
    }

    fn registry(&self) -> &Registry {
        &self.registry
    }
}
