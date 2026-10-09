//! Operaciones de dominio sobre Valkey: sesiones, aceptación de comandos,
//! emisión secuenciada, lease de reenvío y mapeo de incidentes del C5.

use std::{collections::HashMap, time::Duration};

use fred::{
    prelude::*,
    types::{Expiration, Value},
};
use protocol::ServerEvent;
use uuid::Uuid;

use events::RECORD_STREAM;

use crate::valkey::{
    NO_DEDUPE, SWEEP_KEY, SessionKeys, Valkey, ValkeyError, c5_incident_key, value_to_string,
};

/// Une los registros codificados para pasarlos al script (uno por línea).
fn blob(records: &[String]) -> Value {
    s(records.join("\n"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResumeOutcome {
    Resumed { latest_seq: u64 },
    Missing,
    Closed,
    BadToken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptOutcome {
    /// Primera vez que se recibe: ya es durable en Valkey.
    Accepted { echo_seq: Option<u64> },
    /// Reenvío de un mensaje ya aceptado.
    Duplicate,
    /// La sesión ya no existe (expiró).
    SessionMissing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmitOutcome {
    Emitted(u64),
    Duplicate,
    SessionGone,
}

/// Comando pendiente de reenviar al C5.
#[derive(Debug, Clone)]
pub struct PendingCommand {
    pub stream_id: String,
    pub message_json: String,
    pub received_at_ms: i64,
}

/// Un comando del ciudadano listo para aceptarse.
pub struct AcceptCommand<'a> {
    pub client_msg_id: Uuid,
    /// JSON del mensaje ya validado (lo que se reenviará al C5).
    pub message_json: &'a str,
    pub now_ms: i64,
    /// Si va al stream de reenvío al C5 (los `client_event` no).
    pub forward: bool,
    /// Eco secuenciado para el historial (chats).
    pub echo: Option<&'a ServerEvent>,
    /// Registros codificados que se escriben en la misma operación.
    pub records: &'a [String],
}

#[derive(Clone)]
pub struct Store {
    valkey: Valkey,
}

fn s(v: impl ToString) -> Value {
    Value::from(v.to_string())
}

impl Store {
    pub fn new(valkey: Valkey) -> Self {
        Self { valkey }
    }

    pub fn valkey(&self) -> &Valkey {
        &self.valkey
    }

    pub async fn create_session(
        &self,
        session_id: Uuid,
        token_hash: &str,
        now_ms: i64,
        ttl: Duration,
        fields: &[(&str, String)],
        records: &[String],
    ) -> Result<bool, ValkeyError> {
        let keys = SessionKeys::new(session_id);
        let mut all = keys.data();
        all.push(RECORD_STREAM.to_owned());
        let mut args = vec![
            s(session_id),
            s(token_hash),
            s(now_ms),
            s(ttl.as_millis()),
            blob(records),
        ];
        for (field, value) in fields {
            args.push(s(field));
            args.push(s(value));
        }
        let created: i64 = self
            .valkey
            .run(&self.valkey.scripts.session_create, all, args)
            .await?;
        Ok(created == 1)
    }

    pub async fn resume_session(
        &self,
        session_id: Uuid,
        presented_hash: &str,
        new_hash: &str,
        records: &[String],
    ) -> Result<ResumeOutcome, ValkeyError> {
        let keys = SessionKeys::new(session_id);
        let mut all = keys.data();
        all.push(RECORD_STREAM.to_owned());
        let args = vec![s(presented_hash), s(new_hash), blob(records)];
        let reply: Vec<Value> = self
            .valkey
            .run(&self.valkey.scripts.session_resume, all, args)
            .await?;
        let status = reply.first().and_then(value_to_string).unwrap_or_default();
        let seq = reply
            .get(1)
            .and_then(value_to_string)
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        Ok(match status.as_str() {
            "ok" => ResumeOutcome::Resumed { latest_seq: seq },
            "closed" => ResumeOutcome::Closed,
            "bad_token" => ResumeOutcome::BadToken,
            _ => ResumeOutcome::Missing,
        })
    }

    /// Renueva el TTL deslizante de la sesión.
    pub async fn touch(&self, session_id: Uuid) -> Result<bool, ValkeyError> {
        let keys = SessionKeys::new(session_id);
        let mut all = keys.data();
        all.push(keys.fwd.clone());
        let touched: i64 = self
            .valkey
            .run(&self.valkey.scripts.touch, all, Vec::new())
            .await?;
        Ok(touched == 1)
    }

    pub async fn session_fields(
        &self,
        session_id: Uuid,
    ) -> Result<HashMap<String, String>, ValkeyError> {
        Ok(self
            .valkey
            .client
            .hgetall(SessionKeys::new(session_id).sess)
            .await?)
    }

    pub async fn set_fields(
        &self,
        session_id: Uuid,
        fields: &[(&str, String)],
    ) -> Result<(), ValkeyError> {
        if fields.is_empty() {
            return Ok(());
        }
        let map: HashMap<String, String> = fields
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect();
        let _: i64 = self
            .valkey
            .client
            .hset(SessionKeys::new(session_id).sess, map)
            .await?;
        Ok(())
    }

    /// Devuelve `true` si este es el primer valor del campo (HSETNX).
    pub async fn set_field_once(
        &self,
        session_id: Uuid,
        field: &str,
        value: &str,
    ) -> Result<bool, ValkeyError> {
        Ok(self
            .valkey
            .client
            .hsetnx(SessionKeys::new(session_id).sess, field, value)
            .await?)
    }

    pub async fn accept(
        &self,
        session_id: Uuid,
        command: AcceptCommand<'_>,
    ) -> Result<AcceptOutcome, ValkeyError> {
        let AcceptCommand {
            client_msg_id,
            message_json,
            now_ms,
            forward,
            echo,
            records,
        } = command;
        let keys = SessionKeys::new(session_id);
        let mut all = keys.data();
        all.push(keys.fwd.clone());
        all.push(SWEEP_KEY.to_owned());
        all.push(RECORD_STREAM.to_owned());
        let echo_payload = match echo {
            Some(event) => {
                serde_json::to_string(event).map_err(|e| ValkeyError::Unexpected(e.to_string()))?
            }
            None => String::new(),
        };
        let args = vec![
            s(session_id),
            s(client_msg_id),
            s(message_json),
            s(now_ms),
            s(if forward { "1" } else { "0" }),
            s(echo_payload),
            blob(records),
        ];
        let reply: Vec<Value> = self
            .valkey
            .run(&self.valkey.scripts.accept, all, args)
            .await?;
        let status = reply.first().and_then(value_to_string).unwrap_or_default();
        Ok(match status.as_str() {
            "accepted" => {
                let seq = reply
                    .get(2)
                    .and_then(value_to_string)
                    .and_then(|v| v.parse::<u64>().ok());
                AcceptOutcome::Accepted {
                    echo_seq: seq.filter(|s| *s > 0),
                }
            }
            "duplicate" => AcceptOutcome::Duplicate,
            _ => AcceptOutcome::SessionMissing,
        })
    }

    /// Emite un mensaje secuenciado junto con sus registros. `dedupe` =
    /// (llave, ttl) para eventos que pueden llegar repetidos (del C5): si es
    /// repetido, no se emite ni se registra de nuevo.
    pub async fn emit(
        &self,
        session_id: Uuid,
        event: &ServerEvent,
        dedupe: Option<(&str, Duration)>,
        fields: &[(&str, String)],
        records: &[String],
    ) -> Result<EmitOutcome, ValkeyError> {
        let keys = SessionKeys::new(session_id);
        let mut all = keys.data();
        let (dedupe_key, dedupe_ttl) = match dedupe {
            Some((key, ttl)) => (key.to_owned(), ttl),
            None => (NO_DEDUPE.to_owned(), Duration::ZERO),
        };
        all.push(dedupe_key);
        all.push(RECORD_STREAM.to_owned());
        let payload =
            serde_json::to_string(event).map_err(|e| ValkeyError::Unexpected(e.to_string()))?;
        let mut args = vec![
            s(session_id),
            s(payload),
            s(dedupe_ttl.as_millis().max(1)),
            blob(records),
        ];
        for (field, value) in fields {
            args.push(s(field));
            args.push(s(value));
        }
        let seq: i64 = self
            .valkey
            .run(&self.valkey.scripts.emit, all, args)
            .await?;
        Ok(match seq {
            n if n > 0 => EmitOutcome::Emitted(n.unsigned_abs()),
            0 => EmitOutcome::Duplicate,
            _ => EmitOutcome::SessionGone,
        })
    }

    /// Frames secuenciados con `seq > after_seq`, en orden.
    pub async fn replay(
        &self,
        session_id: Uuid,
        after_seq: u64,
    ) -> Result<Vec<(u64, String)>, ValkeyError> {
        let key = SessionKeys::new(session_id).out;
        let start = format!("({after_seq}-0");
        let entries: Vec<(String, HashMap<String, String>)> = self
            .valkey
            .client
            .xrange_values(key, start, "+", None)
            .await?;
        Ok(entries
            .into_iter()
            .filter_map(|(id, mut fields)| {
                let seq = id.split('-').next()?.parse().ok()?;
                Some((seq, fields.remove("f")?))
            })
            .collect())
    }

    pub async fn latest_seq(&self, session_id: Uuid) -> Result<u64, ValkeyError> {
        let seq: Option<u64> = self
            .valkey
            .client
            .get(SessionKeys::new(session_id).seq)
            .await?;
        Ok(seq.unwrap_or(0))
    }

    // --- Reenvío al C5 -----------------------------------------------------

    pub async fn lease_acquire(
        &self,
        session_id: Uuid,
        owner: &str,
        ttl: Duration,
    ) -> Result<bool, ValkeyError> {
        let key = SessionKeys::new(session_id).fwd;
        let got: i64 = self
            .valkey
            .run(
                &self.valkey.scripts.lease_acquire,
                vec![key],
                vec![s(owner), s(ttl.as_millis())],
            )
            .await?;
        Ok(got == 1)
    }

    /// `Ok(true)` si se liberó; `Ok(false)` si quedan pendientes o ya no es dueño.
    pub async fn lease_release_if_drained(
        &self,
        session_id: Uuid,
        owner: &str,
    ) -> Result<ReleaseOutcome, ValkeyError> {
        let keys = SessionKeys::new(session_id);
        let reply: String = self
            .valkey
            .run(
                &self.valkey.scripts.lease_release,
                vec![keys.fwd, keys.sess, keys.inbox, SWEEP_KEY.to_owned()],
                vec![s(owner), s(session_id)],
            )
            .await?;
        Ok(match reply.as_str() {
            "released" => ReleaseOutcome::Released,
            "pending" => ReleaseOutcome::Pending,
            _ => ReleaseOutcome::NotOwner,
        })
    }

    pub async fn mark_forwarded(
        &self,
        session_id: Uuid,
        owner: &str,
        stream_id: &str,
        fields: &[(&str, String)],
    ) -> Result<bool, ValkeyError> {
        let keys = SessionKeys::new(session_id);
        let mut args = vec![s(owner), s(stream_id)];
        for (field, value) in fields {
            args.push(s(field));
            args.push(s(value));
        }
        let ok: i64 = self
            .valkey
            .run(
                &self.valkey.scripts.mark_forwarded,
                vec![keys.fwd, keys.sess],
                args,
            )
            .await?;
        Ok(ok == 1)
    }

    pub async fn pending_commands(
        &self,
        session_id: Uuid,
        after_id: &str,
        count: u64,
    ) -> Result<Vec<PendingCommand>, ValkeyError> {
        let key = SessionKeys::new(session_id).inbox;
        let entries: Vec<(String, HashMap<String, String>)> = self
            .valkey
            .client
            .xrange_values(key, format!("({after_id}"), "+", Some(count))
            .await?;
        Ok(entries
            .into_iter()
            .filter_map(|(stream_id, mut fields)| {
                Some(PendingCommand {
                    stream_id,
                    message_json: fields.remove("m")?,
                    received_at_ms: fields.get("ts").and_then(|t| t.parse().ok()).unwrap_or(0),
                })
            })
            .collect())
    }

    /// Sesiones con comandos pendientes desde antes de `older_than_ms`.
    pub async fn sweep_candidates(
        &self,
        older_than_ms: i64,
        limit: i64,
    ) -> Result<Vec<Uuid>, ValkeyError> {
        let ids: Vec<String> = self
            .valkey
            .client
            .zrangebyscore(SWEEP_KEY, "-inf", older_than_ms, false, Some((0, limit)))
            .await?;
        Ok(ids.iter().filter_map(|id| id.parse().ok()).collect())
    }

    /// Cuántas sesiones tienen comandos sin reenviar (métrica y alerta).
    pub async fn sweep_backlog(&self) -> Result<u64, ValkeyError> {
        Ok(self.valkey.client.zcard(SWEEP_KEY).await?)
    }

    // --- Mapeo con el C5 ---------------------------------------------------

    pub async fn map_c5_incident(
        &self,
        c5_incident_id: &str,
        session_id: Uuid,
        ttl: Duration,
    ) -> Result<(), ValkeyError> {
        let expiration = Expiration::PX(i64::try_from(ttl.as_millis()).unwrap_or(i64::MAX));
        let _: Option<String> = self
            .valkey
            .client
            .set(
                c5_incident_key(c5_incident_id),
                session_id.to_string(),
                Some(expiration),
                None,
                false,
            )
            .await?;
        Ok(())
    }

    pub async fn session_for_c5_incident(
        &self,
        c5_incident_id: &str,
    ) -> Result<Option<Uuid>, ValkeyError> {
        let sid: Option<String> = self
            .valkey
            .client
            .get(c5_incident_key(c5_incident_id))
            .await?;
        Ok(sid.and_then(|s| s.parse().ok()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseOutcome {
    Released,
    Pending,
    NotOwner,
}
