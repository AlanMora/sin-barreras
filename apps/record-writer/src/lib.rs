//! record-writer: guarda el registro completo e íntegro del lado ciudadano.
//!
//! Consume `sb:rec:events` con un consumer group y escribe en Postgres de
//! forma idempotente y encadenada. Confirma (XACK) solo después del COMMIT.
//! Además crea checkpoints periódicos que anclan todos los registros.

pub mod config;
pub mod consumer;

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use axum::Router;
use service_kit::health::{self, CheckStatus, ReadinessProbe};
use tokio::{sync::watch, task::JoinHandle};

use crate::consumer::Consumer;

#[derive(Clone)]
struct Probe(Arc<Consumer>);

impl ReadinessProbe for Probe {
    async fn check(&self) -> BTreeMap<&'static str, CheckStatus> {
        let valkey = match tokio::time::timeout(Duration::from_secs(1), self.0.ping()).await {
            Ok(true) => CheckStatus::Ok,
            _ => CheckStatus::Failing {
                reason: "sin conexión".into(),
            },
        };
        let postgres = match tokio::time::timeout(
            Duration::from_secs(2),
            sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(self.0.writer.pool()),
        )
        .await
        {
            Ok(Ok(_)) => CheckStatus::Ok,
            _ => CheckStatus::Failing {
                reason: "sin conexión".into(),
            },
        };
        BTreeMap::from([("valkey", valkey), ("postgres", postgres)])
    }
}

pub fn router(consumer: Arc<Consumer>) -> Router {
    health::router(Probe(consumer), env!("CARGO_PKG_VERSION"))
}

/// Arranca consumo, reclamo de abandonados y checkpoints.
pub fn spawn(consumer: &Arc<Consumer>, stop: &watch::Receiver<bool>) -> Vec<JoinHandle<()>> {
    vec![
        tokio::spawn(Arc::clone(consumer).run(stop.clone())),
        tokio::spawn(Arc::clone(consumer).run_claimer(stop.clone())),
        tokio::spawn(run_checkpoints(Arc::clone(consumer), stop.clone())),
    ]
}

async fn run_checkpoints(consumer: Arc<Consumer>, mut stop: watch::Receiver<bool>) {
    loop {
        tokio::select! {
            () = tokio::time::sleep(consumer.tuning.checkpoint_every) => {}
            _ = stop.changed() => return,
        }
        match registry::checkpoint::create(consumer.writer.pool()).await {
            Ok(Some(cp)) => tracing::info!(
                checkpoint_id = cp.checkpoint_id,
                registros = cp.records_count,
                "checkpoint creado"
            ),
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "no se pudo crear el checkpoint; se reintentará"),
        }
    }
}
