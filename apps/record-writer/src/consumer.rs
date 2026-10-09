//! Consumo del stream del registro con garantía de no pérdida (ADR-0016).
//!
//! REGLA DE ORO 2:
//! - XACK **solo después** del COMMIT en Postgres.
//! - Falla temporal de Postgres → se reintenta el mismo lote indefinidamente.
//!   Mientras tanto, el writer renueva la posesión de sus pendientes
//!   (`XCLAIM … JUSTID`, que no cuenta como entrega), así una caída larga de
//!   Postgres NUNCA manda eventos a la cola de errores.
//! - Writer muerto → sus pendientes quedan inactivos y otro writer los
//!   reclama (`XPENDING` + `XCLAIM`).
//! - Evento que nunca podrá guardarse (o que excede el máximo de entregas) →
//!   cola de errores `sb:rec:dlq` y XACK. Nada se borra: se puede reinyectar.
//! - Escrituras idempotentes (por `event_id`): reprocesar no duplica.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use events::{DEAD_LETTER_STREAM, RECORD_GROUP, RECORD_STREAM, StreamRecord};
use fred::{
    interfaces::{ClientLike, StreamsInterface},
    prelude::Client,
    types::Value,
};
use protocol::Timestamp;
use registry::writer::{Entry, WriteError, Writer};
use service_kit::backoff::Backoff;
use tokio::sync::watch;

use crate::config::Tuning;

type RawEntry = (String, HashMap<String, String>);

/// Contadores para observabilidad (Fase 8 los expone como métricas).
#[derive(Debug, Default)]
pub struct Stats {
    pub inserted: AtomicU64,
    pub duplicates: AtomicU64,
    pub dead_lettered: AtomicU64,
    pub claimed: AtomicU64,
    pub transient_failures: AtomicU64,
}

pub struct Consumer {
    pub client: Client,
    pub writer: Writer,
    pub consumer: String,
    pub tuning: Tuning,
    pub stats: Stats,
}

impl Consumer {
    /// Crea el consumer group si no existe, desde el PRINCIPIO del stream
    /// (`0`): los eventos publicados antes de que existiera el grupo también
    /// se guardan.
    pub async fn ensure_group(&self) -> Result<(), fred::error::Error> {
        match self
            .client
            .xgroup_create::<Value, _, _, _>(RECORD_STREAM, RECORD_GROUP, "0", true)
            .await
        {
            Ok(_) => Ok(()),
            Err(e) if e.details().contains("BUSYGROUP") => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Bucle principal. Primero procesa los pendientes propios (de una
    /// ejecución anterior con el mismo nombre) y después los nuevos.
    pub async fn run(self: Arc<Self>, mut stop: watch::Receiver<bool>) {
        let mut backoff = Backoff::new(Duration::from_millis(200), Duration::from_secs(10));
        while let Err(error) = self.ensure_group().await {
            tracing::warn!(%error, "no se pudo crear el consumer group; reintentando");
            if wait_or_stop(&mut stop, backoff.next_delay()).await {
                return;
            }
        }
        backoff.reset();
        let mut own_pending_first = true;
        while !*stop.borrow() {
            let (id, block) = if own_pending_first {
                ("0", None)
            } else {
                (">", Some(duration_ms(self.tuning.block)))
            };
            let read = self
                .client
                .xreadgroup_map::<String, String, String, String, _, _, _, _>(
                    RECORD_GROUP,
                    self.consumer.as_str(),
                    Some(self.tuning.batch_size),
                    block,
                    false,
                    RECORD_STREAM,
                    id,
                )
                .await;
            match read {
                Ok(mut streams) => {
                    backoff.reset();
                    let entries = streams.remove(RECORD_STREAM).unwrap_or_default();
                    if entries.is_empty() {
                        own_pending_first = false;
                        continue;
                    }
                    self.process(entries, &mut stop).await;
                }
                Err(error) => {
                    tracing::warn!(%error, "no se pudo leer del stream; reintentando");
                    if wait_or_stop(&mut stop, backoff.next_delay()).await {
                        return;
                    }
                }
            }
        }
        tracing::info!(
            "consumidor detenido; los pendientes no confirmados quedan para el siguiente"
        );
    }

    /// Reclama periódicamente los pendientes de consumidores muertos.
    pub async fn run_claimer(self: Arc<Self>, mut stop: watch::Receiver<bool>) {
        let every = self.tuning.claim_idle / 2;
        loop {
            if wait_or_stop(&mut stop, every).await {
                return;
            }
            if let Err(error) = self.claim_abandoned(&mut stop).await {
                tracing::warn!(%error, "no se pudieron revisar los pendientes abandonados");
            }
        }
    }

    pub async fn claim_abandoned(
        &self,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<(), fred::error::Error> {
        let idle = duration_ms(self.tuning.claim_idle);
        let pending: Vec<(String, String, u64, u64)> = self
            .client
            .xpending(
                RECORD_STREAM,
                RECORD_GROUP,
                (idle, "-", "+", self.tuning.batch_size),
            )
            .await?;
        if pending.is_empty() {
            return Ok(());
        }
        let ids: Vec<String> = pending.iter().map(|(id, ..)| id.clone()).collect();
        let claimed: Vec<RawEntry> = self
            .client
            .xclaim_values(
                RECORD_STREAM,
                RECORD_GROUP,
                self.consumer.as_str(),
                idle,
                ids,
                None,
                None,
                None,
                false,
                false,
            )
            .await?;
        self.stats
            .claimed
            .fetch_add(claimed.len() as u64, Ordering::Relaxed);
        tracing::warn!(
            count = claimed.len(),
            "pendientes de un consumidor inactivo reclamados"
        );
        self.process(claimed, stop).await;
        Ok(())
    }

    /// Guarda un conjunto de entradas. Devuelve cuando todas quedaron
    /// guardadas o en la cola de errores (o si se pide detenerse).
    pub async fn process(&self, entries: Vec<RawEntry>, stop: &mut watch::Receiver<bool>) {
        let deliveries = self.delivery_counts(&entries).await;
        let mut good = Vec::with_capacity(entries.len());
        for (id, mut fields) in entries {
            let raw = fields.remove("e").unwrap_or_default();
            if deliveries
                .get(&id)
                .is_some_and(|n| *n > self.tuning.max_deliveries)
            {
                self.dead_letter(&id, &raw, "excedió el máximo de entregas")
                    .await;
                continue;
            }
            match serde_json::from_str::<StreamRecord>(&raw) {
                Ok(record) => good.push((
                    Entry {
                        stream_id: id,
                        record,
                    },
                    raw,
                )),
                Err(_) => self.dead_letter(&id, &raw, "entrada ilegible").await,
            }
        }
        if good.is_empty() {
            return;
        }
        let batch: Vec<Entry> = good.iter().map(|(e, _)| e.clone()).collect();
        match self.write_with_retry(&batch, stop).await {
            Some(Ok(())) => {}
            Some(Err(_)) => {
                // Hay al menos un evento no guardable: se aísla uno por uno.
                for (entry, raw) in &good {
                    match self
                        .write_with_retry(std::slice::from_ref(entry), stop)
                        .await
                    {
                        Some(Ok(())) | None => {}
                        Some(Err(reason)) => self.dead_letter(&entry.stream_id, raw, &reason).await,
                    }
                }
            }
            None => {}
        }
    }

    /// `Some(Ok)` = guardado y confirmado; `Some(Err(motivo))` = permanente;
    /// `None` = se pidió detenerse (queda pendiente para después).
    async fn write_with_retry(
        &self,
        batch: &[Entry],
        stop: &mut watch::Receiver<bool>,
    ) -> Option<Result<(), String>> {
        let ids: Vec<String> = batch.iter().map(|e| e.stream_id.clone()).collect();
        let mut backoff = Backoff::new(Duration::from_millis(200), Duration::from_secs(10));
        let mut last_refresh = Instant::now();
        loop {
            match self.writer.write_batch(batch).await {
                Ok(outcome) => {
                    self.stats
                        .inserted
                        .fetch_add(outcome.inserted, Ordering::Relaxed);
                    self.stats
                        .duplicates
                        .fetch_add(outcome.duplicates, Ordering::Relaxed);
                    self.ack(&ids).await;
                    return Some(Ok(()));
                }
                Err(WriteError::Permanent { reason, .. }) => return Some(Err(reason)),
                Err(WriteError::Transient(error)) => {
                    self.stats
                        .transient_failures
                        .fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(%error, eventos = batch.len(), "Postgres no disponible; los eventos esperan en el stream");
                    // Renovar la posesión para que nadie los crea abandonados.
                    if last_refresh.elapsed() > self.tuning.claim_idle / 3 {
                        self.refresh_ownership(&ids).await;
                        last_refresh = Instant::now();
                    }
                    if wait_or_stop(stop, backoff.next_delay()).await {
                        return None;
                    }
                }
            }
        }
    }

    async fn refresh_ownership(&self, ids: &[String]) {
        let result: Result<Value, _> = self
            .client
            .xclaim(
                RECORD_STREAM,
                RECORD_GROUP,
                self.consumer.as_str(),
                0,
                ids.to_vec(),
                None,
                None,
                None,
                false,
                true,
            )
            .await;
        if let Err(error) = result {
            tracing::warn!(%error, "no se pudo renovar la posesión de los pendientes");
        }
    }

    /// XACK con reintentos: si falla, el evento se reprocesa (idempotente).
    async fn ack(&self, ids: &[String]) {
        let mut backoff = Backoff::new(Duration::from_millis(100), Duration::from_secs(2));
        for _ in 0..5 {
            match self
                .client
                .xack::<i64, _, _, _>(RECORD_STREAM, RECORD_GROUP, ids.to_vec())
                .await
            {
                Ok(_) => return,
                Err(error) => {
                    tracing::warn!(%error, "XACK falló; reintentando");
                    tokio::time::sleep(backoff.next_delay()).await;
                }
            }
        }
        tracing::error!(
            eventos = ids.len(),
            "XACK no confirmado: se reprocesarán sin duplicarse"
        );
    }

    async fn dead_letter(&self, id: &str, raw: &str, reason: &str) {
        let now = Timestamp::now().to_string();
        let fields = vec![
            ("source_id", id),
            ("e", raw),
            ("reason", reason),
            ("failed_at", now.as_str()),
            ("consumer", self.consumer.as_str()),
        ];
        let mut backoff = Backoff::new(Duration::from_millis(100), Duration::from_secs(5));
        // Primero a la cola de errores, después XACK: si algo falla entre
        // ambos, a lo mucho queda duplicado en la cola, nunca perdido.
        loop {
            match self
                .client
                .xadd::<String, _, _, _, _>(DEAD_LETTER_STREAM, false, None, "*", fields.clone())
                .await
            {
                Ok(_) => break,
                Err(error) => {
                    tracing::warn!(%error, "no se pudo escribir en la cola de errores; reintentando");
                    tokio::time::sleep(backoff.next_delay()).await;
                }
            }
        }
        self.stats.dead_lettered.fetch_add(1, Ordering::Relaxed);
        tracing::error!(
            stream_id = id,
            reason,
            "evento enviado a la cola de errores"
        );
        self.ack(&[id.to_owned()]).await;
    }

    async fn delivery_counts(&self, entries: &[RawEntry]) -> HashMap<String, u64> {
        let (Some((first, _)), Some((last, _))) = (entries.first(), entries.last()) else {
            return HashMap::new();
        };
        let pending: Result<Vec<(String, String, u64, u64)>, _> = self
            .client
            .xpending(
                RECORD_STREAM,
                RECORD_GROUP,
                (first.as_str(), last.as_str(), entries.len() as u64),
            )
            .await;
        pending
            .map(|p| p.into_iter().map(|(id, _, _, n)| (id, n)).collect())
            .unwrap_or_default()
    }

    /// Eventos aún no guardados (pendientes + no leídos) y en la cola de errores.
    pub async fn backlog(&self) -> Result<(u64, u64), fred::error::Error> {
        let groups: Vec<HashMap<String, Value>> = self.client.xinfo_groups(RECORD_STREAM).await?;
        let lag = groups
            .iter()
            .find(|g| g.get("name").and_then(|v| v.as_str()).as_deref() == Some(RECORD_GROUP))
            .map(|g| {
                let num = |k: &str| g.get(k).and_then(Value::as_u64).unwrap_or(0);
                num("lag") + num("pending")
            })
            .unwrap_or(0);
        let dlq: u64 = self.client.xlen(DEAD_LETTER_STREAM).await?;
        Ok((lag, dlq))
    }

    pub async fn ping(&self) -> bool {
        self.client.ping::<String>(None).await.is_ok()
    }
}

fn duration_ms(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

/// Espera `delay` o hasta que se pida detenerse. `true` = detenerse.
async fn wait_or_stop(stop: &mut watch::Receiver<bool>, delay: Duration) -> bool {
    if *stop.borrow() {
        return true;
    }
    tokio::select! {
        () = tokio::time::sleep(delay) => *stop.borrow(),
        _ = stop.changed() => true,
    }
}
