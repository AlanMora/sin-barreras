//! Adaptador en memoria del C5 para pruebas: registra las llamadas, permite
//! inyectar fallas y empujar eventos como si fueran del operador.

use std::{
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
};

use futures::{FutureExt, StreamExt, future::BoxFuture, stream::BoxStream};
use tokio::sync::broadcast;
use tokio_stream_shim::BroadcastStream;
use uuid::Uuid;

use super::{
    C5Error, C5Gateway, C5InboundEvent, LocationReport, NewIncident, OutboundMessage,
    VideoCallRequest,
};

/// Llamada registrada por el adaptador.
#[derive(Debug, Clone, PartialEq)]
pub enum Call {
    CreateIncident(NewIncident),
    SendMessage(OutboundMessage),
    UpdateLocation(LocationReport),
    RequestVideoCall(VideoCallRequest),
}

pub struct InMemoryGateway {
    calls: Mutex<Vec<Call>>,
    /// Respuestas idempotentes ya dadas: idempotency_key → id del C5.
    idempotent: Mutex<HashMap<Uuid, String>>,
    fail_next_transient: AtomicU32,
    reject_next: AtomicU32,
    counter: AtomicU64,
    events: broadcast::Sender<C5InboundEvent>,
}

impl Default for InMemoryGateway {
    fn default() -> Self {
        let (events, _) = broadcast::channel(1024);
        Self {
            calls: Mutex::default(),
            idempotent: Mutex::default(),
            fail_next_transient: AtomicU32::new(0),
            reject_next: AtomicU32::new(0),
            counter: AtomicU64::new(0),
            events,
        }
    }
}

impl InMemoryGateway {
    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().map(|c| c.clone()).unwrap_or_default()
    }

    /// Las próximas `n` llamadas fallan con error temporal.
    pub fn fail_transiently(&self, n: u32) {
        self.fail_next_transient.store(n, Ordering::SeqCst);
    }

    /// Las próximas `n` llamadas se rechazan de forma definitiva.
    pub fn reject(&self, n: u32) {
        self.reject_next.store(n, Ordering::SeqCst);
    }

    /// Empuja un evento como si viniera del C5.
    pub fn push_event(&self, event: C5InboundEvent) {
        let _ = self.events.send(event);
    }

    fn handle(&self, call: Call, key: Uuid, prefix: &str) -> Result<String, C5Error> {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(call);
        }
        let take = |counter: &AtomicU32| {
            counter
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                .is_ok()
        };
        if take(&self.fail_next_transient) {
            return Err(C5Error::Transient("falla simulada".into()));
        }
        if take(&self.reject_next) {
            return Err(C5Error::Rejected("rechazo simulado".into()));
        }
        let mut idempotent = self
            .idempotent
            .lock()
            .map_err(|_| C5Error::Transient("lock".into()))?;
        let id = idempotent
            .entry(key)
            .or_insert_with(|| {
                format!(
                    "{prefix}-{:06}",
                    self.counter.fetch_add(1, Ordering::SeqCst) + 1
                )
            })
            .clone();
        Ok(id)
    }
}

impl C5Gateway for InMemoryGateway {
    fn create_incident(&self, request: NewIncident) -> BoxFuture<'_, Result<String, C5Error>> {
        let key = request.idempotency_key;
        let result = self.handle(Call::CreateIncident(request), key, "C5INC");
        async move { result }.boxed()
    }

    fn send_message(&self, message: OutboundMessage) -> BoxFuture<'_, Result<String, C5Error>> {
        let key = message.idempotency_key;
        let result = self.handle(Call::SendMessage(message), key, "C5MSG");
        async move { result }.boxed()
    }

    fn update_location(&self, report: LocationReport) -> BoxFuture<'_, Result<(), C5Error>> {
        let key = report.idempotency_key;
        let result = self
            .handle(Call::UpdateLocation(report), key, "C5LOC")
            .map(|_| ());
        async move { result }.boxed()
    }

    fn request_video_call(
        &self,
        request: VideoCallRequest,
    ) -> BoxFuture<'_, Result<String, C5Error>> {
        let key = request.idempotency_key;
        let result = self.handle(Call::RequestVideoCall(request), key, "C5VID");
        async move { result }.boxed()
    }

    fn events(&self) -> BoxStream<'static, C5InboundEvent> {
        BroadcastStream::new(self.events.subscribe()).boxed()
    }
}

/// Adaptador mínimo de `broadcast::Receiver` a `Stream` (evita depender de
/// `tokio-stream` solo para esto). Ignora los mensajes perdidos por retraso.
mod tokio_stream_shim {
    use futures::Stream;
    use tokio::sync::broadcast;

    pub struct BroadcastStream;

    impl BroadcastStream {
        #[allow(clippy::new_ret_no_self)]
        pub fn new<T: Clone + Send + 'static>(rx: broadcast::Receiver<T>) -> impl Stream<Item = T> {
            futures::stream::unfold(rx, |mut rx| async move {
                loop {
                    match rx.recv().await {
                        Ok(item) => return Some((item, rx)),
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => return None,
                    }
                }
            })
        }
    }
}
