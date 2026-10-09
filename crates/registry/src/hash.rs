//! Codificación canónica y hash SHA-256 de un registro.
//!
//! La codificación es binaria, con longitud prefijada y orden fijo de campos:
//! no depende de cómo Postgres o JSON representen los datos, así que cualquier
//! herramienta puede recalcularla. El hash cubre el TEXTO CIFRADO (no el
//! claro): el borrado criptográfico de una sesión no rompe la cadena.

use sha2::{Digest, Sha256};
use uuid::Uuid;

const DOMAIN: &[u8] = b"sin-barreras/record/v1";
const CHECKPOINT_DOMAIN: &[u8] = b"sin-barreras/checkpoint/v1";

/// Todo lo que entra al hash de un registro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashedFields<'a> {
    pub event_id: Uuid,
    pub schema_version: i16,
    pub chain_id: &'a str,
    pub chain_seq: i64,
    pub kind: &'a str,
    pub occurred_at_ms: i64,
    pub recorded_at_ms: i64,
    pub client_ts_ms: Option<i64>,
    pub session_id: Option<Uuid>,
    pub incident_id: Option<Uuid>,
    pub c5_incident_id: Option<&'a str>,
    pub producer: &'a str,
    pub trace_id: Option<&'a str>,
    pub stream_id: &'a str,
    pub attributes_json: &'a str,
    pub sealed_alg: Option<&'a str>,
    pub sealed_key_ref: Option<&'a str>,
    pub sealed_nonce: Option<&'a [u8]>,
    pub sealed_ciphertext: Option<&'a [u8]>,
    pub prev_hash: &'a [u8],
}

struct Encoder(Sha256);

impl Encoder {
    fn bytes(&mut self, value: &[u8]) {
        let len = u64::try_from(value.len()).unwrap_or(u64::MAX);
        self.0.update(len.to_be_bytes());
        self.0.update(value);
    }
    fn opt(&mut self, value: Option<&[u8]>) {
        match value {
            None => self.0.update([0]),
            Some(v) => {
                self.0.update([1]);
                self.bytes(v);
            }
        }
    }
    fn i64(&mut self, value: i64) {
        self.0.update(value.to_be_bytes());
    }
}

pub fn record_hash(f: &HashedFields<'_>) -> [u8; 32] {
    let mut e = Encoder(Sha256::new());
    e.bytes(DOMAIN);
    e.bytes(f.event_id.as_bytes());
    e.i64(i64::from(f.schema_version));
    e.bytes(f.chain_id.as_bytes());
    e.i64(f.chain_seq);
    e.bytes(f.kind.as_bytes());
    e.i64(f.occurred_at_ms);
    e.i64(f.recorded_at_ms);
    e.opt(
        f.client_ts_ms
            .map(i64::to_be_bytes)
            .as_ref()
            .map(<[u8; 8]>::as_slice),
    );
    e.opt(f.session_id.as_ref().map(|u| u.as_bytes().as_slice()));
    e.opt(f.incident_id.as_ref().map(|u| u.as_bytes().as_slice()));
    e.opt(f.c5_incident_id.map(str::as_bytes));
    e.bytes(f.producer.as_bytes());
    e.opt(f.trace_id.map(str::as_bytes));
    e.bytes(f.stream_id.as_bytes());
    e.bytes(f.attributes_json.as_bytes());
    e.opt(f.sealed_alg.map(str::as_bytes));
    e.opt(f.sealed_key_ref.map(str::as_bytes));
    e.opt(f.sealed_nonce);
    e.opt(f.sealed_ciphertext);
    e.bytes(f.prev_hash);
    e.0.finalize().into()
}

/// Raíz de un lote de registros: hash de (record_id, hash) en orden.
#[derive(Default)]
pub struct BatchRoot {
    hasher: Sha256,
    count: u64,
}

impl BatchRoot {
    pub fn push(&mut self, record_id: i64, hash: &[u8]) {
        self.hasher.update(record_id.to_be_bytes());
        self.hasher.update(hash);
        self.count += 1;
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    pub fn finish(self) -> [u8; 32] {
        self.hasher.finalize().into()
    }
}

pub fn checkpoint_hash(
    created_at_ms: i64,
    covered_from: i64,
    covered_until: i64,
    records_count: i64,
    batch_root: &[u8],
    prev_hash: &[u8],
) -> [u8; 32] {
    let mut e = Encoder(Sha256::new());
    e.bytes(CHECKPOINT_DOMAIN);
    e.i64(created_at_ms);
    e.i64(covered_from);
    e.i64(covered_until);
    e.i64(records_count);
    e.bytes(batch_root);
    e.bytes(prev_hash);
    e.0.finalize().into()
}

/// JSON canónico: llaves ordenadas, sin espacios (serde_json con `Map`
/// ordenado por defecto, ya que no se activa `preserve_order`).
pub fn canonical_json(value: &serde_json::Value) -> String {
    fn sort(value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => {
                let mut entries: Vec<_> = map.iter().collect();
                entries.sort_by(|a, b| a.0.cmp(b.0));
                serde_json::Value::Object(
                    entries
                        .into_iter()
                        .map(|(k, v)| (k.clone(), sort(v)))
                        .collect(),
                )
            }
            serde_json::Value::Array(items) => {
                serde_json::Value::Array(items.iter().map(sort).collect())
            }
            other => other.clone(),
        }
    }
    sort(value).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> HashedFields<'static> {
        HashedFields {
            event_id: Uuid::from_u128(1),
            schema_version: 1,
            chain_id: "s1",
            chain_seq: 1,
            kind: "citizen_message",
            occurred_at_ms: 1_000,
            recorded_at_ms: 2_000,
            client_ts_ms: None,
            session_id: Some(Uuid::from_u128(2)),
            incident_id: None,
            c5_incident_id: None,
            producer: "bff/a",
            trace_id: None,
            stream_id: "1-0",
            attributes_json: "{}",
            sealed_alg: Some("A256GCM"),
            sealed_key_ref: Some("{}"),
            sealed_nonce: Some(&[1, 2, 3]),
            sealed_ciphertext: Some(&[4, 5, 6]),
            prev_hash: &[0; 32],
        }
    }

    #[test]
    fn hash_is_deterministic_and_sensitive_to_every_field() {
        let base = record_hash(&sample());
        assert_eq!(base, record_hash(&sample()));
        let variants = [
            HashedFields {
                chain_seq: 2,
                ..sample()
            },
            HashedFields {
                kind: "operator_message",
                ..sample()
            },
            HashedFields {
                occurred_at_ms: 1_001,
                ..sample()
            },
            HashedFields {
                client_ts_ms: Some(0),
                ..sample()
            },
            HashedFields {
                incident_id: Some(Uuid::from_u128(3)),
                ..sample()
            },
            HashedFields {
                attributes_json: "{\"a\":1}",
                ..sample()
            },
            HashedFields {
                sealed_ciphertext: Some(&[4, 5, 7]),
                ..sample()
            },
            HashedFields {
                prev_hash: &[1; 32],
                ..sample()
            },
        ];
        for v in variants {
            assert_ne!(record_hash(&v), base);
        }
    }

    #[test]
    fn none_and_empty_are_different() {
        let a = HashedFields {
            trace_id: None,
            ..sample()
        };
        let b = HashedFields {
            trace_id: Some(""),
            ..sample()
        };
        assert_ne!(record_hash(&a), record_hash(&b));
    }

    #[test]
    fn canonical_json_sorts_keys_recursively() {
        let v: serde_json::Value =
            serde_json::from_str(r#"{"b":1,"a":{"d":[{"z":1,"y":2}],"c":null}}"#).unwrap();
        assert_eq!(
            canonical_json(&v),
            r#"{"a":{"c":null,"d":[{"y":2,"z":1}]},"b":1}"#
        );
    }
}
