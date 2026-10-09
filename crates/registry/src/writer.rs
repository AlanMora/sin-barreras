//! Inserción idempotente y encadenada de los eventos del registro.
//!
//! - **Idempotente**: si el `event_id` ya existe, se omite (un reintento tras
//!   una caída entre el COMMIT y el XACK no duplica nada).
//! - **Encadenada**: cada registro guarda el hash del anterior de su cadena.
//!   Un candado por cadena (`pg_advisory_xact_lock`) serializa a varios
//!   writers; se toman en orden para evitar interbloqueos.
//! - **Coordinada con los checkpoints**: los writers toman un candado global
//!   compartido; el checkpoint lo toma exclusivo y así nunca "salta" un
//!   registro a medio escribir.

use std::collections::{BTreeSet, HashMap};

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use events::{EventKind, StreamRecord};
use protocol::Timestamp;
use sqlx::{PgPool, Postgres, Row, Transaction};
use time::OffsetDateTime;

use crate::{
    GENESIS_HASH,
    hash::{HashedFields, canonical_json, record_hash},
};

/// Candado global compartido por writers y exclusivo para checkpoints.
pub const GLOBAL_LOCK: i64 = 0x5B_5EC0_0001;

#[derive(Debug, Clone)]
pub struct Entry {
    /// Id de la entrada en el stream de Valkey.
    pub stream_id: String,
    pub record: StreamRecord,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BatchOutcome {
    pub inserted: u64,
    pub duplicates: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// Postgres no disponible, timeout, interbloqueo…: se reintenta el lote.
    #[error("falla temporal de Postgres: {0}")]
    Transient(#[source] sqlx::Error),
    /// El lote contiene un evento que nunca podrá guardarse tal cual.
    #[error("evento no guardable ({stream_id}): {reason}")]
    Permanent { stream_id: String, reason: String },
}

fn classify(stream_id: &str, error: sqlx::Error) -> WriteError {
    if let sqlx::Error::Database(db) = &error
        && let Some(code) = db.code()
    {
        // 22xxx: dato inválido; 23xxx: restricción violada (excepto 23505,
        // que solo ocurriría por una carrera y se resuelve reintentando).
        // 42xxx (permisos, esquema) NO es permanente: es configuración y
        // mandar todo a la cola de errores sería peor.
        if code.starts_with("22") || (code.starts_with("23") && code != "23505") {
            return WriteError::Permanent {
                stream_id: stream_id.to_owned(),
                reason: db.message().to_owned(),
            };
        }
    }
    WriteError::Transient(error)
}

fn ts(ms: i64) -> Result<OffsetDateTime, String> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000).map_err(|e| e.to_string())
}

/// Cadena a la que pertenece un evento.
pub fn chain_id(record: &StreamRecord) -> String {
    match record.event.session_id {
        Some(session) => session.to_string(),
        None => {
            let date = record.event.occurred_at.as_offset_date_time().date();
            format!("system-{date}")
        }
    }
}

/// Columnas ya preparadas de un registro.
struct Prepared {
    attributes_json: String,
    sealed_alg: Option<String>,
    sealed_key_ref: Option<String>,
    sealed_nonce: Option<Vec<u8>>,
    sealed_ciphertext: Option<Vec<u8>>,
}

fn prepare(entry: &Entry) -> Result<Prepared, String> {
    if entry.record.event.sensitive.is_some() {
        // Nunca debe llegar contenido en claro: el BFF cifra antes de publicar.
        return Err("el evento trae contenido sensible sin cifrar".into());
    }
    let attributes_json = canonical_json(&serde_json::Value::Object(
        entry.record.event.attributes.clone(),
    ));
    let (sealed_alg, sealed_key_ref, sealed_nonce, sealed_ciphertext) = match &entry.record.sealed {
        None => (None, None, None, None),
        Some(sealed) => {
            let key_ref = serde_json::to_value(&sealed.key_ref).map_err(|e| e.to_string())?;
            let nonce = B64
                .decode(&sealed.nonce)
                .map_err(|_| "nonce no es base64".to_owned())?;
            let ciphertext = B64
                .decode(&sealed.ciphertext)
                .map_err(|_| "ciphertext no es base64".to_owned())?;
            (
                Some(sealed.alg.clone()),
                Some(canonical_json(&key_ref)),
                Some(nonce),
                Some(ciphertext),
            )
        }
    };
    Ok(Prepared {
        attributes_json,
        sealed_alg,
        sealed_key_ref,
        sealed_nonce,
        sealed_ciphertext,
    })
}

#[derive(Clone)]
pub struct Writer {
    pool: PgPool,
}

impl Writer {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Guarda un lote en UNA transacción: o se guarda todo, o nada.
    pub async fn write_batch(&self, entries: &[Entry]) -> Result<BatchOutcome, WriteError> {
        let first = entries
            .first()
            .map(|e| e.stream_id.clone())
            .unwrap_or_default();
        // Validar antes de abrir la transacción: un evento malformado se
        // reporta como permanente con su propio id.
        let mut prepared = Vec::with_capacity(entries.len());
        for entry in entries {
            let p = prepare(entry).map_err(|reason| WriteError::Permanent {
                stream_id: entry.stream_id.clone(),
                reason,
            })?;
            prepared.push(p);
        }

        let mut tx = self.pool.begin().await.map_err(|e| classify(&first, e))?;
        sqlx::query("SELECT pg_advisory_xact_lock_shared($1)")
            .bind(GLOBAL_LOCK)
            .execute(&mut *tx)
            .await
            .map_err(|e| classify(&first, e))?;
        // Candados por cadena en orden alfabético (evita interbloqueos).
        let chains: BTreeSet<String> = entries.iter().map(|e| chain_id(&e.record)).collect();
        for chain in &chains {
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 5317))")
                .bind(chain)
                .execute(&mut *tx)
                .await
                .map_err(|e| classify(&first, e))?;
        }

        let mut heads: HashMap<String, (i64, Vec<u8>)> = HashMap::new();
        let mut outcome = BatchOutcome::default();
        for (entry, prep) in entries.iter().zip(prepared) {
            if insert_one(&mut tx, entry, prep, &mut heads)
                .await
                .map_err(|e| classify(&entry.stream_id, e))?
            {
                outcome.inserted += 1;
            } else {
                outcome.duplicates += 1;
            }
        }
        tx.commit().await.map_err(|e| classify(&first, e))?;
        Ok(outcome)
    }
}

/// Inserta un registro. `Ok(false)` si ya existía.
async fn insert_one(
    tx: &mut Transaction<'_, Postgres>,
    entry: &Entry,
    prep: Prepared,
    heads: &mut HashMap<String, (i64, Vec<u8>)>,
) -> Result<bool, sqlx::Error> {
    let event = &entry.record.event;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM registry.records WHERE event_id = $1)")
            .bind(event.event_id)
            .fetch_one(&mut **tx)
            .await?;
    if exists {
        return Ok(false);
    }

    let chain = chain_id(&entry.record);
    let (prev_seq, prev_hash) = match heads.get(&chain) {
        Some(head) => head.clone(),
        None => {
            let row = sqlx::query(
                "SELECT chain_seq, hash FROM registry.records WHERE chain_id = $1 ORDER BY chain_seq DESC LIMIT 1",
            )
            .bind(&chain)
            .fetch_optional(&mut **tx)
            .await?;
            match row {
                Some(row) => (
                    row.try_get::<i64, _>("chain_seq")?,
                    row.try_get::<Vec<u8>, _>("hash")?,
                ),
                None => (0, GENESIS_HASH.to_vec()),
            }
        }
    };

    let chain_seq = prev_seq + 1;
    let recorded_at_ms = Timestamp::now().unix_millis();
    let kind = event.kind.as_str();
    let schema_version = i16::try_from(event.schema_version).unwrap_or(i16::MAX);
    let hash = record_hash(&HashedFields {
        event_id: event.event_id,
        schema_version,
        chain_id: &chain,
        chain_seq,
        kind,
        occurred_at_ms: event.occurred_at.unix_millis(),
        recorded_at_ms,
        client_ts_ms: event.client_ts.map(|t| t.unix_millis()),
        session_id: event.session_id,
        incident_id: event.incident_id,
        c5_incident_id: event.c5_incident_id.as_deref(),
        producer: &event.producer,
        trace_id: event.trace_id.as_deref(),
        stream_id: &entry.stream_id,
        attributes_json: &prep.attributes_json,
        sealed_alg: prep.sealed_alg.as_deref(),
        sealed_key_ref: prep.sealed_key_ref.as_deref(),
        sealed_nonce: prep.sealed_nonce.as_deref(),
        sealed_ciphertext: prep.sealed_ciphertext.as_deref(),
        prev_hash: &prev_hash,
    });

    let to_ts = |ms: i64| ts(ms).map_err(|e| sqlx::Error::Decode(e.into()));
    sqlx::query(
        "INSERT INTO registry.records (event_id, schema_version, chain_id, chain_seq, kind, occurred_at,
             recorded_at, client_ts, session_id, incident_id, c5_incident_id, producer, trace_id, stream_id,
             attributes_json, sealed_alg, sealed_key_ref, sealed_nonce, sealed_ciphertext, prev_hash, hash)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21)",
    )
    .bind(event.event_id)
    .bind(schema_version)
    .bind(&chain)
    .bind(chain_seq)
    .bind(kind)
    .bind(to_ts(event.occurred_at.unix_millis())?)
    .bind(to_ts(recorded_at_ms)?)
    .bind(event.client_ts.map(|t| to_ts(t.unix_millis())).transpose()?)
    .bind(event.session_id)
    .bind(event.incident_id)
    .bind(event.c5_incident_id.as_deref())
    .bind(&event.producer)
    .bind(event.trace_id.as_deref())
    .bind(&entry.stream_id)
    .bind(&prep.attributes_json)
    .bind(prep.sealed_alg.as_deref())
    .bind(prep.sealed_key_ref.as_deref())
    .bind(prep.sealed_nonce.as_deref())
    .bind(prep.sealed_ciphertext.as_deref())
    .bind(prev_hash.as_slice())
    .bind(hash.as_slice())
    .execute(&mut **tx)
    .await?;

    if event.kind == EventKind::SessionKeyCreated {
        insert_session_key(tx, entry).await?;
    }
    heads.insert(chain, (chain_seq, hash.to_vec()));
    Ok(true)
}

async fn insert_session_key(
    tx: &mut Transaction<'_, Postgres>,
    entry: &Entry,
) -> Result<(), sqlx::Error> {
    let event = &entry.record.event;
    let attr = |k: &str| {
        event
            .attributes
            .get(k)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned()
    };
    let decode = |k: &str| {
        B64.decode(attr(k))
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))
    };
    let Some(session_id) = event.session_id else {
        return Ok(());
    };
    sqlx::query(
        "INSERT INTO registry.session_keys (session_id, recipient_key_id, alg, encapped, wrapped_key, created_at, source_event_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT DO NOTHING",
    )
    .bind(session_id)
    .bind(attr("recipient_key_id"))
    .bind(attr("alg"))
    .bind(decode("encapped")?)
    .bind(decode("wrapped_key")?)
    .bind(ts(event.occurred_at.unix_millis()).map_err(|e| sqlx::Error::Decode(e.into()))?)
    .bind(event.event_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
