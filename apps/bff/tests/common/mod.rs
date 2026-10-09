//! Utilidades de prueba: Valkey real en contenedor, BFF en su propio runtime
//! (para poder "matarlo" de golpe) y un cliente WebSocket.

#![allow(dead_code, clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use bff::{
    AppState,
    c5::memory::InMemoryGateway,
    config::{C5Config, Config, SessionConfig, ValkeyConfig},
    recorder::MemoryRecorder,
    valkey::Valkey,
};
use futures::{SinkExt, StreamExt};
use protocol::{SequencedMessage, ServerControl, ServerEvent, ServerFrame, Welcome};
use serde_json::{Value, json};
use testcontainers_modules::{
    testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner},
    valkey::Valkey as ValkeyImage,
};
use tokio::sync::oneshot;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message};
use uuid::Uuid;

pub const QR_KEY: [u8; 32] = [7; 32];

pub async fn start_valkey() -> (ContainerAsync<ValkeyImage>, String) {
    let container = ValkeyImage::default()
        .with_tag("9.1-alpine")
        .start()
        .await
        .expect("iniciar Valkey");
    let port = container
        .get_host_port_ipv4(6379)
        .await
        .expect("puerto de Valkey");
    (container, format!("valkey://127.0.0.1:{port}"))
}

/// Un BFF corriendo en su propio runtime. Al soltarlo, el runtime se destruye
/// con todas sus tareas: equivale a matar el proceso.
pub struct TestBff {
    pub addr: SocketAddr,
    pub gateway: Arc<InMemoryGateway>,
    pub recorder: Arc<MemoryRecorder>,
    pub state: Arc<AppState>,
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl TestBff {
    pub async fn start(valkey_url: &str, replica_id: &str, gateway: Arc<InMemoryGateway>) -> Self {
        let config = Config {
            listen_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
            replica_id: replica_id.to_owned(),
            valkey: ValkeyConfig {
                url: valkey_url.to_owned(),
                username: None,
                password: None,
                ca_file: None,
            },
            c5: C5Config {
                mock_url: String::new(),
                mock_api_key: String::new(),
                request_timeout: Duration::from_secs(1),
            },
            sessions: SessionConfig {
                idle_ttl: Duration::from_secs(120),
                incident_ttl: Duration::from_secs(600),
            },
            qr_keys: HashMap::from([("1".to_owned(), QR_KEY.to_vec())]),
            allowed_origins: vec!["http://localhost:5173".to_owned()],
            forward_lease_ttl: Duration::from_secs(2),
        };
        let recorder = Arc::new(MemoryRecorder::default());
        let (ready_tx, ready_rx) = oneshot::channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let gw = Arc::clone(&gateway);
        let rec = Arc::clone(&recorder);
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("runtime");
            runtime.block_on(async move {
                let valkey = Valkey::connect(&config.valkey)
                    .await
                    .expect("conectar a Valkey");
                let state = AppState::new(config, valkey, gw, rec);
                let _bg = bff::spawn_background(&state);
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                    .await
                    .expect("bind");
                let addr = listener.local_addr().expect("addr");
                let app = bff::router(Arc::clone(&state))
                    .into_make_service_with_connect_info::<SocketAddr>();
                tokio::spawn(async move {
                    let _ = axum::serve(listener, app).await;
                });
                let _ = ready_tx.send((addr, state));
                let _ = shutdown_rx.await;
            });
            // Destruye todas las tareas sin esperar (como un proceso que muere).
            runtime.shutdown_background();
        });
        let (addr, state) = ready_rx.await.expect("BFF listo");
        Self {
            addr,
            gateway,
            recorder,
            state,
            shutdown: Some(shutdown_tx),
            thread: Some(thread),
        }
    }

    /// Mata la réplica de golpe.
    pub fn kill(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for TestBff {
    fn drop(&mut self) {
        self.stop();
    }
}

type Ws = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// Cliente WebSocket de prueba que habla el protocolo v1.
pub struct Client {
    ws: Ws,
    pub last_seq: u64,
    pub received: Vec<SequencedMessage>,
}

impl Client {
    pub async fn connect(addr: SocketAddr) -> Self {
        let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
            .await
            .expect("conectar WS");
        Self {
            ws,
            last_seq: 0,
            received: Vec::new(),
        }
    }

    pub async fn send(&mut self, value: Value) {
        self.ws
            .send(Message::Text(value.to_string().into()))
            .await
            .expect("enviar");
    }

    pub async fn send_raw(&mut self, text: &str) {
        self.ws
            .send(Message::Text(text.to_owned().into()))
            .await
            .expect("enviar");
    }

    /// Siguiente frame del servidor (falla tras 5 s).
    pub async fn recv(&mut self) -> ServerFrame {
        loop {
            let message = tokio::time::timeout(Duration::from_secs(5), self.ws.next())
                .await
                .expect("timeout esperando frame")
                .expect("conexión cerrada")
                .expect("error de WS");
            if let Message::Text(text) = message {
                let frame: ServerFrame = serde_json::from_str(&text)
                    .unwrap_or_else(|e| panic!("frame inválido {text}: {e}"));
                if let ServerFrame::Sequenced(m) = &frame {
                    self.last_seq = self.last_seq.max(m.seq);
                    self.received.push(m.clone());
                }
                return frame;
            }
        }
    }

    /// Espera hasta recibir un frame que cumpla `pred` (ignora el resto).
    pub async fn recv_until(&mut self, pred: impl Fn(&ServerFrame) -> bool) -> ServerFrame {
        loop {
            let frame = self.recv().await;
            if pred(&frame) {
                return frame;
            }
        }
    }

    pub async fn recv_event(&mut self, pred: impl Fn(&ServerEvent) -> bool) -> SequencedMessage {
        match self
            .recv_until(|f| matches!(f, ServerFrame::Sequenced(m) if pred(&m.event)))
            .await
        {
            ServerFrame::Sequenced(m) => m,
            ServerFrame::Control(_) => unreachable!(),
        }
    }

    pub async fn recv_control(&mut self) -> ServerControl {
        match self
            .recv_until(|f| matches!(f, ServerFrame::Control(_)))
            .await
        {
            ServerFrame::Control(c) => c,
            ServerFrame::Sequenced(_) => unreachable!(),
        }
    }

    /// `true` si no llega ningún frame durante `wait`.
    pub async fn is_silent(&mut self, wait: Duration) -> bool {
        tokio::time::timeout(wait, self.ws.next()).await.is_err()
    }

    pub async fn hello(&mut self, extra: Value) -> Welcome {
        let mut hello = json!({
            "type": "hello",
            "client_msg_id": Uuid::new_v4(),
            "protocol_version": 1,
            "app_version": "0.1.0",
            "language": "es-MX",
        });
        if let (Some(obj), Some(extra)) = (hello.as_object_mut(), extra.as_object()) {
            for (k, v) in extra {
                obj.insert(k.clone(), v.clone());
            }
        }
        self.send(hello).await;
        match self.recv_control().await {
            ServerControl::Welcome(w) => w,
            other => panic!("se esperaba welcome, llegó {other:?}"),
        }
    }

    pub async fn resume(&mut self, welcome: &Welcome, last_seq: u64) -> Welcome {
        self.last_seq = last_seq;
        self.hello(json!({ "resume": {
            "session_id": welcome.session_id,
            "session_token": welcome.session_token,
            "last_seq": last_seq,
        }}))
        .await
    }

    /// Envía un comando y devuelve el `ack` correspondiente.
    pub async fn command(&mut self, mut value: Value) -> (Uuid, ServerControl) {
        let id = Uuid::new_v4();
        value["client_msg_id"] = json!(id);
        self.send(value).await;
        let ack = self
            .recv_until(|f| match f {
                ServerFrame::Control(ServerControl::Ack(a)) => a.client_msg_id == id,
                ServerFrame::Control(ServerControl::Error(e)) => e.client_msg_id == Some(id),
                _ => false,
            })
            .await;
        match ack {
            ServerFrame::Control(c) => (id, c),
            ServerFrame::Sequenced(_) => unreachable!(),
        }
    }

    pub async fn close(mut self) {
        let _ = self.ws.close(None).await;
    }
}

pub fn chat(text: &str) -> Value {
    json!({ "type": "chat_message", "kind": "text", "text": text })
}

pub fn create_incident(channel: &str) -> Value {
    json!({ "type": "create_incident", "channel": channel })
}

/// Espera (hasta `timeout`) a que `cond` sea verdadera.
pub async fn eventually(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    cond()
}
