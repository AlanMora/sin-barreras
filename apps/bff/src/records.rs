//! Registro completo desde el BFF (ver ADR-0014 y ADR-0015).
//!
//! Dos caminos hacia el stream `sb:rec:events`:
//! 1. **Atómico** ([`Records::encode`]): los eventos de contenido (mensajes,
//!    ubicaciones, estados, llave de la sesión) se escriben DENTRO del mismo
//!    script Lua que acepta o emite el mensaje. No existe un instante en que
//!    el mensaje exista y su registro no.
//! 2. **Asíncrono** ([`Records::record`]): eventos técnicos (conexiones, acks,
//!    errores). Un canal acotado y una tarea los publican sin bloquear nunca al
//!    ciudadano (REGLA DE ORO 1). Si Valkey falla, se reintenta.
//!
//! En ambos casos el contenido sensible se cifra aquí, antes de salir del
//! proceso. El BFF solo tiene la llave PÚBLICA de investigación.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use crypto::{DataKey, OperationalKey, RecipientPublicKey, unwrap_operational, wrap_for_recipient};
use events::{
    EventKind, KeyRef, RECORD_STREAM, RecordEvent, SealedPayload, StreamRecord, WrappedKeyData,
    inline_key_context, sensitive_aad, session_key_context,
};
use fred::prelude::*;
use service_kit::backoff::Backoff;
use tokio::sync::mpsc;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::valkey::SessionKeys;

/// Capacidad del canal asíncrono. Se dimensiona para varios minutos de
/// eventos técnicos con Valkey caído; si se llena, se cuenta y se alerta.
const ASYNC_CAPACITY: usize = 100_000;
/// DEK en caché por réplica (las sesiones activas).
const DEK_CACHE_MAX: usize = 50_000;

/// Marcador que el script Lua sustituye por el `seq` real.
pub const SEQ_PLACEHOLDER: &str = "__SEQ__";

#[derive(Debug, thiserror::Error)]
pub enum RecordError {
    #[error(transparent)]
    Crypto(#[from] crypto::CryptoError),
    #[error("no se pudo serializar el evento: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Clone, Debug)]
pub struct RecordKeys {
    /// Llave pública de investigación activa.
    pub recipient: RecipientPublicKey,
    /// Llaves operativas; la primera es la activa (las demás, para rotar).
    pub operational: Vec<OperationalKey>,
}

/// Llave de una sesión nueva.
pub struct NewSessionKey {
    /// DEK envuelta con la llave operativa (se guarda en la sesión en Valkey).
    pub operational_wrapped: String,
    /// Evento `session_key_created` con la DEK envuelta para investigación.
    pub event: RecordEvent,
}

pub struct Records {
    keys: RecordKeys,
    client: Client,
    producer: String,
    deks: Mutex<HashMap<Uuid, DataKey>>,
    tx: mpsc::Sender<RecordEvent>,
    dropped: AtomicU64,
}

impl Records {
    /// Crea el registro y arranca la tarea de publicación asíncrona.
    pub fn start(keys: RecordKeys, client: Client, producer: String) -> Arc<Self> {
        let (tx, rx) = mpsc::channel(ASYNC_CAPACITY);
        let records = Arc::new(Self {
            keys,
            client,
            producer,
            deks: Mutex::new(HashMap::new()),
            tx,
            dropped: AtomicU64::new(0),
        });
        tokio::spawn(run_async_writer(Arc::clone(&records), rx));
        records
    }

    pub fn producer(&self) -> &str {
        &self.producer
    }

    /// Eventos técnicos descartados por canal lleno (debe ser siempre 0).
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Camino asíncrono. Nunca bloquea ni falla hacia quien llama.
    pub fn record(&self, event: RecordEvent) {
        if let Err(error) = self.tx.try_send(event) {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            let kind = match &error {
                mpsc::error::TrySendError::Full(e) | mpsc::error::TrySendError::Closed(e) => {
                    e.kind.as_str()
                }
            };
            tracing::error!(kind, "canal del registro lleno: evento técnico descartado");
        }
    }

    /// Genera la DEK de una sesión nueva.
    pub fn new_session_key(&self, session_id: Uuid) -> Result<NewSessionKey, RecordError> {
        let dek = DataKey::generate()?;
        let context = session_key_context(session_id);
        let wrapped = wrap_for_recipient(&self.keys.recipient, &dek, &context)?;
        let active = self
            .keys
            .operational
            .first()
            .ok_or(crypto::CryptoError::BadKey("sin llave operativa"))?;
        let operational_wrapped = active.wrap(&dek, &context)?;
        self.cache(session_id, dek);
        let event = RecordEvent::new(EventKind::SessionKeyCreated, self.producer.clone())
            .session(session_id)
            .attr("recipient_key_id", wrapped.recipient_key_id)
            .attr("alg", wrapped.alg)
            .attr("encapped", B64.encode(&wrapped.encapped))
            .attr("wrapped_key", B64.encode(&wrapped.ciphertext));
        Ok(NewSessionKey {
            operational_wrapped,
            event,
        })
    }

    /// Codifica un evento para el stream, cifrando su contenido sensible.
    pub async fn encode(&self, mut event: RecordEvent) -> Result<String, RecordError> {
        let sealed = match event.sensitive.take() {
            None => None,
            Some(payload) => {
                let plaintext = Zeroizing::new(serde_json::to_vec(&payload)?);
                let aad = sensitive_aad(&event);
                let session_dek = match event.session_id {
                    Some(sid) => self
                        .session_dek(sid)
                        .await
                        .map(|k| (k, KeyRef::Session { session_id: sid })),
                    None => None,
                };
                let (key, key_ref) = match session_dek {
                    Some(found) => found,
                    // Sin DEK de sesión disponible: llave propia del evento,
                    // envuelta para investigación dentro del mismo registro.
                    None => self.inline_key(event.event_id)?,
                };
                let sealed = crypto::seal(&key, &aad, &plaintext)?;
                Some(SealedPayload {
                    alg: crypto::DATA_ALG.to_owned(),
                    key_ref,
                    nonce: B64.encode(sealed.nonce),
                    ciphertext: B64.encode(&sealed.ciphertext),
                })
            }
        };
        Ok(serde_json::to_string(&StreamRecord { event, sealed })?)
    }

    /// Codifica varios eventos; un fallo se registra y no impide los demás.
    pub async fn encode_all(&self, events: Vec<RecordEvent>) -> Vec<String> {
        let mut out = Vec::with_capacity(events.len());
        for event in events {
            let kind = event.kind.as_str();
            match self.encode(event).await {
                Ok(json) => out.push(json),
                Err(error) => {
                    tracing::error!(%error, kind, "no se pudo codificar un evento del registro")
                }
            }
        }
        out
    }

    fn inline_key(&self, event_id: Uuid) -> Result<(DataKey, KeyRef), RecordError> {
        let dek = DataKey::generate()?;
        let wrapped =
            wrap_for_recipient(&self.keys.recipient, &dek, &inline_key_context(event_id))?;
        let data = WrappedKeyData {
            alg: wrapped.alg,
            recipient_key_id: wrapped.recipient_key_id,
            encapped: B64.encode(&wrapped.encapped),
            ciphertext: B64.encode(&wrapped.ciphertext),
        };
        Ok((dek, KeyRef::Inline { wrapped: data }))
    }

    async fn session_dek(&self, session_id: Uuid) -> Option<DataKey> {
        if let Some(dek) = self
            .deks
            .lock()
            .ok()
            .and_then(|d| d.get(&session_id).cloned())
        {
            return Some(dek);
        }
        let wrapped: Option<String> = self
            .client
            .hget(SessionKeys::new(session_id).sess, "dek")
            .await
            .ok()?;
        let dek = unwrap_operational(
            &self.keys.operational,
            &wrapped?,
            &session_key_context(session_id),
        )
        .ok()?;
        self.cache(session_id, dek.clone());
        Some(dek)
    }

    fn cache(&self, session_id: Uuid, dek: DataKey) {
        if let Ok(mut deks) = self.deks.lock() {
            if deks.len() >= DEK_CACHE_MAX {
                // Simple y seguro: se vacía y se recarga desde Valkey bajo demanda.
                deks.clear();
            }
            deks.insert(session_id, dek);
        }
    }
}

async fn run_async_writer(records: Arc<Records>, mut rx: mpsc::Receiver<RecordEvent>) {
    let mut batch = Vec::with_capacity(256);
    while let Some(first) = rx.recv().await {
        batch.push(first);
        while batch.len() < 256 {
            match rx.try_recv() {
                Ok(event) => batch.push(event),
                Err(_) => break,
            }
        }
        let encoded = records.encode_all(std::mem::take(&mut batch)).await;
        let mut backoff = Backoff::new(Duration::from_millis(100), Duration::from_secs(5));
        let mut pending = encoded.as_slice();
        while let Some((entry, rest)) = pending.split_first() {
            let result: Result<String, _> = records
                .client
                .xadd(RECORD_STREAM, false, None, "*", vec![("e", entry.as_str())])
                .await;
            match result {
                Ok(_) => {
                    pending = rest;
                    backoff.reset();
                }
                Err(error) => {
                    tracing::warn!(%error, pendientes = pending.len(), "no se pudo publicar en el registro; reintentando");
                    tokio::time::sleep(backoff.next_delay()).await;
                }
            }
        }
    }
}
