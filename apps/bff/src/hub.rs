//! Conexiones WebSocket locales de esta réplica y despacho del bus.
//!
//! Cada réplica escucha el canal `sb:bus`; cuando llega un mensaje para una
//! sesión conectada aquí, lo entrega a su(s) conexión(es). Si el canal de una
//! conexión está lleno, el mensaje se descarta sin bloquear: la conexión lo
//! recupera con su verificación periódica de `seq` (ver `ws.rs`).

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct LiveFrame {
    pub seq: u64,
    pub frame: Arc<str>,
}

type Connections = HashMap<Uuid, HashMap<u64, mpsc::Sender<LiveFrame>>>;

#[derive(Default)]
pub struct Hub {
    connections: Mutex<Connections>,
    next_id: AtomicU64,
}

/// Al soltarse, retira la conexión del hub.
pub struct Registration {
    hub: Arc<Hub>,
    session_id: Uuid,
    conn_id: u64,
}

impl Drop for Registration {
    fn drop(&mut self) {
        if let Ok(mut conns) = self.hub.connections.lock()
            && let Some(per_session) = conns.get_mut(&self.session_id)
        {
            per_session.remove(&self.conn_id);
            if per_session.is_empty() {
                conns.remove(&self.session_id);
            }
        }
    }
}

impl Hub {
    pub fn register(
        self: &Arc<Self>,
        session_id: Uuid,
    ) -> (Registration, mpsc::Receiver<LiveFrame>) {
        let (tx, rx) = mpsc::channel(256);
        let conn_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut conns) = self.connections.lock() {
            conns.entry(session_id).or_default().insert(conn_id, tx);
        }
        (
            Registration {
                hub: Arc::clone(self),
                session_id,
                conn_id,
            },
            rx,
        )
    }

    /// Entrega un frame a las conexiones locales de la sesión.
    pub fn dispatch(&self, session_id: Uuid, frame: LiveFrame) {
        let senders: Vec<_> = match self.connections.lock() {
            Ok(conns) => conns
                .get(&session_id)
                .map(|m| m.values().cloned().collect())
                .unwrap_or_default(),
            Err(_) => return,
        };
        for sender in senders {
            if sender.try_send(frame.clone()).is_err() {
                tracing::warn!(%session_id, seq = frame.seq, "canal de conexión lleno; se recuperará por seq");
            }
        }
    }

    pub fn active_connections(&self) -> usize {
        self.connections
            .lock()
            .map(|c| c.values().map(HashMap::len).sum())
            .unwrap_or(0)
    }

    /// Interpreta un mensaje del bus: `<session_id>|<seq>|<frame>`.
    pub fn parse_bus_message(raw: &str) -> Option<(Uuid, LiveFrame)> {
        let mut parts = raw.splitn(3, '|');
        let session_id = parts.next()?.parse().ok()?;
        let seq = parts.next()?.parse().ok()?;
        let frame = parts.next()?;
        Some((
            session_id,
            LiveFrame {
                seq,
                frame: Arc::from(frame),
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dispatches_only_to_registered_session() {
        let hub = Arc::new(Hub::default());
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let (_reg_a, mut rx_a) = hub.register(a);
        let (_reg_b, mut rx_b) = hub.register(b);
        hub.dispatch(
            a,
            LiveFrame {
                seq: 1,
                frame: Arc::from("{}"),
            },
        );
        assert_eq!(rx_a.recv().await.unwrap().seq, 1);
        assert!(rx_b.try_recv().is_err());
    }

    #[tokio::test]
    async fn unregisters_on_drop() {
        let hub = Arc::new(Hub::default());
        let (reg, _rx) = hub.register(Uuid::new_v4());
        assert_eq!(hub.active_connections(), 1);
        drop(reg);
        assert_eq!(hub.active_connections(), 0);
    }

    #[test]
    fn parses_bus_messages() {
        let sid = Uuid::new_v4();
        let (parsed_sid, frame) =
            Hub::parse_bus_message(&format!("{sid}|42|{{\"seq\":42,\"a\":\"x|y\"}}")).unwrap();
        assert_eq!(parsed_sid, sid);
        assert_eq!(frame.seq, 42);
        assert_eq!(&*frame.frame, "{\"seq\":42,\"a\":\"x|y\"}");
        assert!(Hub::parse_bus_message("basura").is_none());
    }
}
