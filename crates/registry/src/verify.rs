//! Verificación completa de la integridad del registro.
//!
//! Detecta:
//! - Contenido alterado (el hash recalculado no coincide).
//! - Registros borrados, insertados o reordenados dentro de una cadena
//!   (`chain_seq` no contiguo o `prev_hash` que no apunta al anterior).
//! - Cadenas completas borradas o registros agregados fuera de orden
//!   (la raíz de un checkpoint no coincide).
//! - Checkpoints alterados o borrados (su propia cadena se rompe).
//!
//! Solo necesita permisos de lectura (`sb_auditor`); el contenido sigue cifrado.

use futures::TryStreamExt;
use serde::Serialize;
use sqlx::{PgPool, Row, postgres::PgRow};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    GENESIS_HASH,
    hash::{BatchRoot, HashedFields, checkpoint_hash, record_hash},
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Issue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_id: Option<i64>,
    pub problem: String,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct Report {
    pub records: u64,
    pub chains: u64,
    pub checkpoints: u64,
    /// Registros posteriores al último checkpoint (aún no anclados).
    pub records_after_last_checkpoint: u64,
    pub issues: Vec<Issue>,
}

impl Report {
    pub fn is_ok(&self) -> bool {
        self.issues.is_empty()
    }
}

fn ms(t: OffsetDateTime) -> i64 {
    i64::try_from(t.unix_timestamp_nanos() / 1_000_000).unwrap_or(i64::MAX)
}

fn record_issue(row: &PgRow, chain: &str, problem: impl Into<String>) -> Issue {
    Issue {
        record_id: row.try_get("record_id").ok(),
        chain_id: Some(chain.to_owned()),
        checkpoint_id: None,
        problem: problem.into(),
    }
}

/// Verifica todo el registro (o una sola cadena si se indica).
pub async fn verify(pool: &PgPool, only_chain: Option<&str>) -> Result<Report, sqlx::Error> {
    let mut report = Report::default();
    verify_chains(pool, only_chain, &mut report).await?;
    if only_chain.is_none() {
        verify_checkpoints(pool, &mut report).await?;
    }
    Ok(report)
}

async fn verify_chains(
    pool: &PgPool,
    only_chain: Option<&str>,
    report: &mut Report,
) -> Result<(), sqlx::Error> {
    let mut rows = sqlx::query(
        "SELECT record_id, event_id, schema_version, chain_id, chain_seq, kind, occurred_at, recorded_at,
                client_ts, session_id, incident_id, c5_incident_id, producer, trace_id, stream_id,
                attributes_json, sealed_alg, sealed_key_ref, sealed_nonce, sealed_ciphertext, prev_hash, hash
         FROM registry.records
         WHERE $1::text IS NULL OR chain_id = $1
         ORDER BY chain_id, chain_seq",
    )
    .bind(only_chain)
    .fetch(pool);

    let mut current_chain: Option<String> = None;
    let mut expected_seq = 1_i64;
    let mut previous_hash = GENESIS_HASH.to_vec();
    while let Some(row) = rows.try_next().await? {
        report.records += 1;
        let chain: String = row.try_get("chain_id")?;
        if current_chain.as_deref() != Some(chain.as_str()) {
            current_chain = Some(chain.clone());
            report.chains += 1;
            expected_seq = 1;
            previous_hash = GENESIS_HASH.to_vec();
        }
        let chain_seq: i64 = row.try_get("chain_seq")?;
        let prev_hash: Vec<u8> = row.try_get("prev_hash")?;
        let stored_hash: Vec<u8> = row.try_get("hash")?;

        if chain_seq != expected_seq {
            report.issues.push(record_issue(
                &row,
                &chain,
                format!("secuencia rota: se esperaba {expected_seq} y hay {chain_seq} (registros faltantes)"),
            ));
        }
        if prev_hash != previous_hash {
            report.issues.push(record_issue(
                &row,
                &chain,
                "prev_hash no coincide con el registro anterior",
            ));
        }

        let event_id: Uuid = row.try_get("event_id")?;
        let attributes_json: String = row.try_get("attributes_json")?;
        let sealed_nonce: Option<Vec<u8>> = row.try_get("sealed_nonce")?;
        let sealed_ciphertext: Option<Vec<u8>> = row.try_get("sealed_ciphertext")?;
        let sealed_alg: Option<String> = row.try_get("sealed_alg")?;
        let sealed_key_ref: Option<String> = row.try_get("sealed_key_ref")?;
        let kind: String = row.try_get("kind")?;
        let producer: String = row.try_get("producer")?;
        let stream_id: String = row.try_get("stream_id")?;
        let c5_incident_id: Option<String> = row.try_get("c5_incident_id")?;
        let trace_id: Option<String> = row.try_get("trace_id")?;
        let computed = record_hash(&HashedFields {
            event_id,
            schema_version: row.try_get("schema_version")?,
            chain_id: &chain,
            chain_seq,
            kind: &kind,
            occurred_at_ms: ms(row.try_get("occurred_at")?),
            recorded_at_ms: ms(row.try_get("recorded_at")?),
            client_ts_ms: row
                .try_get::<Option<OffsetDateTime>, _>("client_ts")?
                .map(ms),
            session_id: row.try_get("session_id")?,
            incident_id: row.try_get("incident_id")?,
            c5_incident_id: c5_incident_id.as_deref(),
            producer: &producer,
            trace_id: trace_id.as_deref(),
            stream_id: &stream_id,
            attributes_json: &attributes_json,
            sealed_alg: sealed_alg.as_deref(),
            sealed_key_ref: sealed_key_ref.as_deref(),
            sealed_nonce: sealed_nonce.as_deref(),
            sealed_ciphertext: sealed_ciphertext.as_deref(),
            prev_hash: &prev_hash,
        });
        if computed.as_slice() != stored_hash.as_slice() {
            report.issues.push(record_issue(
                &row,
                &chain,
                "el contenido no coincide con su hash (registro alterado)",
            ));
        }
        expected_seq = chain_seq + 1;
        previous_hash = stored_hash;
    }
    Ok(())
}

async fn verify_checkpoints(pool: &PgPool, report: &mut Report) -> Result<(), sqlx::Error> {
    let checkpoints = sqlx::query(
        "SELECT checkpoint_id, created_at, covered_from, covered_until, records_count, batch_root, prev_hash, hash
         FROM registry.checkpoints ORDER BY checkpoint_id",
    )
    .fetch_all(pool)
    .await?;

    let mut previous_hash = GENESIS_HASH.to_vec();
    let mut previous_until = 0_i64;
    for cp in &checkpoints {
        report.checkpoints += 1;
        let id: i64 = cp.try_get("checkpoint_id")?;
        let issue = |problem: String| Issue {
            record_id: None,
            chain_id: None,
            checkpoint_id: Some(id),
            problem,
        };
        let covered_from: i64 = cp.try_get("covered_from")?;
        let covered_until: i64 = cp.try_get("covered_until")?;
        let records_count: i64 = cp.try_get("records_count")?;
        let batch_root: Vec<u8> = cp.try_get("batch_root")?;
        let prev_hash: Vec<u8> = cp.try_get("prev_hash")?;
        let stored_hash: Vec<u8> = cp.try_get("hash")?;

        if prev_hash != previous_hash || covered_from != previous_until {
            report.issues.push(issue(
                "la cadena de checkpoints está rota (checkpoint borrado o alterado)".into(),
            ));
        }
        let computed = checkpoint_hash(
            ms(cp.try_get("created_at")?),
            covered_from,
            covered_until,
            records_count,
            &batch_root,
            &prev_hash,
        );
        if computed.as_slice() != stored_hash.as_slice() {
            report
                .issues
                .push(issue("el checkpoint fue alterado".into()));
        }

        let mut root = BatchRoot::default();
        let mut rows =
            sqlx::query("SELECT record_id, hash FROM registry.records WHERE record_id > $1 AND record_id <= $2 ORDER BY record_id")
                .bind(covered_from)
                .bind(covered_until)
                .fetch(pool);
        while let Some(row) = rows.try_next().await? {
            root.push(
                row.try_get("record_id")?,
                &row.try_get::<Vec<u8>, _>("hash")?,
            );
        }
        let count = i64::try_from(root.count()).unwrap_or(i64::MAX);
        if count != records_count {
            report.issues.push(issue(format!(
                "el rango ({covered_from}, {covered_until}] tenía {records_count} registros y hoy tiene {count}"
            )));
        } else if root.finish().as_slice() != batch_root.as_slice() {
            report.issues.push(issue(format!(
                "los registros del rango ({covered_from}, {covered_until}] cambiaron"
            )));
        }
        previous_hash = stored_hash;
        previous_until = covered_until;
    }

    let after: i64 =
        sqlx::query_scalar("SELECT count(*) FROM registry.records WHERE record_id > $1")
            .bind(previous_until)
            .fetch_one(pool)
            .await?;
    report.records_after_last_checkpoint = u64::try_from(after).unwrap_or(0);
    Ok(())
}
