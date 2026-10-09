//! Estado en memoria del simulador (se pierde al reiniciar: es solo para desarrollo).

use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
};

use protocol::{IncidentState, Timestamp};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::config::{Chaos, Config};

/// Máximo de eventos que se conservan para reenviar a feeds que reconectan.
const EVENT_BUFFER: usize = 10_000;

#[derive(Debug, Clone, Serialize)]
pub struct Incident {
    pub incident_id: String,
    pub external_ref: Uuid,
    pub channel: String,
    pub language: String,
    pub entry_point: Option<String>,
    pub entry_verified: bool,
    pub status: IncidentState,
    pub created_at: Timestamp,
    pub citizen_messages: u32,
    pub locations: Vec<Value>,
}

pub struct MockState {
    pub config: Config,
    /// Identificador de este arranque: los ids nunca se repiten entre reinicios.
    pub boot: String,
    pub chaos: Mutex<Chaos>,
    inner: Mutex<Inner>,
    pub feed: broadcast::Sender<String>,
}

#[derive(Default)]
struct Inner {
    incidents: HashMap<String, Incident>,
    /// idempotency_key → respuesta ya dada.
    idempotent: HashMap<(&'static str, Uuid), Value>,
    events: VecDeque<(u64, String)>,
    next_event: u64,
    next_incident: u64,
    next_message: u64,
}

impl MockState {
    pub fn new(config: Config) -> Self {
        let (feed, _) = broadcast::channel(1024);
        let chaos = Mutex::new(config.chaos);
        let boot = format!("{:04X}", (random_unit() * 65_536.0) as u32);
        Self {
            config,
            boot,
            chaos,
            inner: Mutex::default(),
            feed,
        }
    }

    pub fn chaos(&self) -> Chaos {
        self.chaos.lock().map(|c| *c).unwrap_or_default()
    }

    /// Devuelve la respuesta previa si la llave ya se usó; si no, ejecuta
    /// `create` y la guarda.
    pub fn idempotent(
        &self,
        operation: &'static str,
        key: Uuid,
        create: impl FnOnce(&mut IdGen<'_>) -> Value,
    ) -> (Value, bool) {
        let Ok(mut inner) = self.inner.lock() else {
            return (json!({}), false);
        };
        if let Some(previous) = inner.idempotent.get(&(operation, key)) {
            return (previous.clone(), false);
        }
        let response = {
            let Inner {
                next_incident,
                next_message,
                ..
            } = &mut *inner;
            create(&mut IdGen {
                boot: &self.boot,
                next_incident,
                next_message,
            })
        };
        inner.idempotent.insert((operation, key), response.clone());
        (response, true)
    }

    pub fn insert_incident(&self, incident: Incident) {
        if let Ok(mut inner) = self.inner.lock() {
            inner
                .incidents
                .insert(incident.incident_id.clone(), incident);
        }
    }

    pub fn with_incident<R>(&self, id: &str, f: impl FnOnce(&mut Incident) -> R) -> Option<R> {
        self.inner.lock().ok()?.incidents.get_mut(id).map(f)
    }

    pub fn incidents(&self) -> Vec<Incident> {
        self.inner
            .lock()
            .map(|i| i.incidents.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Publica un evento en el feed y lo guarda para reenvíos.
    pub fn publish(&self, incident_id: &str, body: Value) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        inner.next_event += 1;
        let seq = inner.next_event;
        let mut event = json!({
            "event_seq": seq,
            "event_id": format!("EVT-{}-{seq:08}", self.boot),
            "incident_id": incident_id,
            "occurred_at": Timestamp::now(),
        });
        if let (Some(event), Some(body)) = (event.as_object_mut(), body.as_object()) {
            event.extend(body.clone());
        }
        let text = event.to_string();
        inner.events.push_back((seq, text.clone()));
        while inner.events.len() > EVENT_BUFFER {
            inner.events.pop_front();
        }
        let _ = self.feed.send(text);
    }

    /// Eventos con `event_seq > since`.
    pub fn events_since(&self, since: u64) -> Vec<String> {
        self.inner
            .lock()
            .map(|i| {
                i.events
                    .iter()
                    .filter(|(seq, _)| *seq > since)
                    .map(|(_, e)| e.clone())
                    .collect()
            })
            .unwrap_or_default()
    }
}

pub struct IdGen<'a> {
    boot: &'a str,
    next_incident: &'a mut u64,
    next_message: &'a mut u64,
}

impl IdGen<'_> {
    pub fn incident(&mut self) -> String {
        *self.next_incident += 1;
        format!("C5M-{}-{:06}", self.boot, self.next_incident)
    }

    pub fn message(&mut self) -> String {
        *self.next_message += 1;
        format!("MSG-{}-{:08}", self.boot, self.next_message)
    }
}

/// Número aleatorio en [0, 1).
pub fn random_unit() -> f64 {
    let mut bytes = [0_u8; 8];
    let _ = getrandom::fill(&mut bytes);
    (u64::from_le_bytes(bytes) >> 11) as f64 / (1_u64 << 53) as f64
}

pub fn random_between(min: u64, max: u64) -> u64 {
    if max <= min {
        return min;
    }
    let span = max - min;
    min + ((random_unit() * span as f64) as u64).min(span)
}
