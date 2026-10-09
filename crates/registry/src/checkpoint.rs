//! Checkpoints globales: encadenan todos los registros por rangos de
//! `record_id`. Si alguien borra una sesión completa (con todos sus
//! registros), la cadena de esa sesión no lo detecta, pero el checkpoint sí.

use futures::TryStreamExt;
use protocol::Timestamp;
use sqlx::{PgPool, Row};
use time::OffsetDateTime;

use crate::{
    GENESIS_HASH,
    hash::{BatchRoot, checkpoint_hash},
    writer::GLOBAL_LOCK,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointInfo {
    pub checkpoint_id: i64,
    pub covered_until: i64,
    pub records_count: i64,
}

/// Crea un checkpoint con los registros nuevos desde el anterior. Devuelve
/// `None` si no hay registros nuevos.
pub async fn create(pool: &PgPool) -> Result<Option<CheckpointInfo>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    // Exclusivo: espera a que terminen las escrituras en curso.
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(GLOBAL_LOCK)
        .execute(&mut *tx)
        .await?;

    let last = sqlx::query(
        "SELECT covered_until, hash FROM registry.checkpoints ORDER BY checkpoint_id DESC LIMIT 1",
    )
    .fetch_optional(&mut *tx)
    .await?;
    let (covered_from, prev_hash) = match last {
        Some(row) => (
            row.try_get::<i64, _>("covered_until")?,
            row.try_get::<Vec<u8>, _>("hash")?,
        ),
        None => (0, GENESIS_HASH.to_vec()),
    };

    let mut root = BatchRoot::default();
    let mut covered_until = covered_from;
    {
        let mut rows = sqlx::query(
            "SELECT record_id, hash FROM registry.records WHERE record_id > $1 ORDER BY record_id",
        )
        .bind(covered_from)
        .fetch(&mut *tx);
        while let Some(row) = rows.try_next().await? {
            let record_id: i64 = row.try_get("record_id")?;
            let hash: Vec<u8> = row.try_get("hash")?;
            root.push(record_id, &hash);
            covered_until = record_id;
        }
    }
    if root.count() == 0 {
        tx.rollback().await?;
        return Ok(None);
    }

    let records_count = i64::try_from(root.count()).unwrap_or(i64::MAX);
    let batch_root = root.finish();
    let created_at_ms = Timestamp::now().unix_millis();
    let hash = checkpoint_hash(
        created_at_ms,
        covered_from,
        covered_until,
        records_count,
        &batch_root,
        &prev_hash,
    );
    let created_at =
        OffsetDateTime::from_unix_timestamp_nanos(i128::from(created_at_ms) * 1_000_000)
            .map_err(|e| sqlx::Error::Decode(e.into()))?;
    let checkpoint_id: i64 = sqlx::query_scalar(
        "INSERT INTO registry.checkpoints (created_at, covered_from, covered_until, records_count, batch_root, prev_hash, hash)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING checkpoint_id",
    )
    .bind(created_at)
    .bind(covered_from)
    .bind(covered_until)
    .bind(records_count)
    .bind(batch_root.as_slice())
    .bind(prev_hash.as_slice())
    .bind(hash.as_slice())
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some(CheckpointInfo {
        checkpoint_id,
        covered_until,
        records_count,
    }))
}
