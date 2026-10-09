//! Transporte WebSocket (protocolo v1, ver `docs/contrato-eventos.md`).
//!
//! Ciclo de vida de una conexión:
//! 1. El primer mensaje debe ser `hello` (10 s máximo). Se negocia versión.
//! 2. Se crea o reanuda la sesión y se responde `welcome` con un token nuevo.
//! 3. Se reenvían los mensajes secuenciados que el cliente no tiene.
//! 4. Bucle: comandos → `session::handle_command` (aceptación durable) → `ack`;
//!    mensajes del bus → cliente; verificación periódica de `seq`.

use std::{
    collections::HashSet,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    extract::{
        ConnectInfo, State, WebSocketUpgrade,
        ws::{CloseFrame, Message, Utf8Bytes, WebSocket},
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use events::{EventKind, RecordEvent};
use futures::{SinkExt, StreamExt, stream::SplitSink};
use protocol::{
    ErrorCode, HEARTBEAT_INTERVAL_MS, MAX_CLIENT_FRAME_BYTES, PROTOCOL_VERSION, ServerControl,
};
use uuid::Uuid;

use crate::{
    AppState,
    forwarder::producer,
    hub::LiveFrame,
    session::{self, CommandReply, NetworkInfo, error},
};

const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
/// Sin ningún mensaje del cliente durante este tiempo, la conexión se cierra
/// (el cliente reconectará y recuperará por `seq`).
const IDLE_TIMEOUT: Duration = Duration::from_millis(3 * HEARTBEAT_INTERVAL_MS as u64);
const SERVER_PING_EVERY: Duration = Duration::from_secs(20);
const CATCH_UP_EVERY: Duration = Duration::from_secs(5);

// Códigos de cierre propios (rango 4000-4999 de la RFC 6455).
const CLOSE_PROTOCOL: u16 = 4400;
const CLOSE_HELLO_TIMEOUT: u16 = 4408;
const CLOSE_IDLE: u16 = 4409;
const CLOSE_SESSION_GONE: u16 = 4410;
const CLOSE_UNSUPPORTED_VERSION: u16 = 4426;
const CLOSE_RATE_LIMITED: u16 = 4429;
const CLOSE_UNAVAILABLE: u16 = 4503;

pub async fn handler(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    if !session::origin_allowed(&state.config.allowed_origins, &headers) {
        return (StatusCode::FORBIDDEN, "origen no permitido").into_response();
    }
    let network = NetworkInfo::from_request(&headers, peer);
    ws.max_message_size(MAX_CLIENT_FRAME_BYTES)
        .max_frame_size(MAX_CLIENT_FRAME_BYTES)
        .on_upgrade(move |socket| Connection::run(state, socket, network))
}

type Sink = SplitSink<WebSocket, Message>;

struct Connection {
    state: Arc<AppState>,
    session_id: Uuid,
    sink: Sink,
    /// `seq` ya enviados en esta conexión.
    sent: HashSet<u64>,
    /// Todo `seq` ≤ `floor` ya está en el cliente (o se le envió).
    floor: u64,
}

impl Connection {
    async fn run(state: Arc<AppState>, socket: WebSocket, network: NetworkInfo) {
        let started = Instant::now();
        let (mut sink, mut stream) = socket.split();

        // 1. hello
        let hello = match tokio::time::timeout(HELLO_TIMEOUT, stream.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => session::parse_hello(&text),
            Ok(_) => Err((
                ErrorCode::HelloRequired,
                "se esperaba hello".to_owned(),
                None,
            )),
            Err(_) => {
                close(&mut sink, CLOSE_HELLO_TIMEOUT, "hello timeout").await;
                return;
            }
        };
        let (hello, dropped) = match hello {
            Ok(h) => h,
            Err((code, message, client_msg_id)) => {
                let _ = send_control(&mut sink, error(code, message, client_msg_id, false)).await;
                close(&mut sink, CLOSE_PROTOCOL, "hello inválido").await;
                return;
            }
        };
        if hello.protocol_version != PROTOCOL_VERSION {
            let _ = send_control(&mut sink, session::unsupported_version(&hello)).await;
            close(&mut sink, CLOSE_UNSUPPORTED_VERSION, "versión no soportada").await;
            return;
        }

        // 2. sesión
        let established = match session::establish_session(&state, &hello, &dropped, &network).await
        {
            Ok(e) => e,
            Err(e) if e.downcast_ref::<session::RateLimitedError>().is_some() => {
                let reply = error(
                    ErrorCode::RateLimited,
                    "demasiadas sesiones; reintenta".into(),
                    Some(hello.client_msg_id),
                    true,
                );
                let _ = send_control(&mut sink, reply).await;
                close(&mut sink, CLOSE_RATE_LIMITED, "demasiadas sesiones").await;
                return;
            }
            Err(e) => {
                tracing::error!(error = %e, "no se pudo establecer la sesión");
                let reply = error(
                    ErrorCode::Internal,
                    "servicio no disponible".into(),
                    Some(hello.client_msg_id),
                    true,
                );
                let _ = send_control(&mut sink, reply).await;
                close(&mut sink, CLOSE_UNAVAILABLE, "servicio no disponible").await;
                return;
            }
        };
        let session_id = established.session_id;

        // Registrarse en el hub ANTES de leer el estado: nada se pierde entre
        // la lectura y la suscripción (los duplicados se descartan por `seq`).
        let (_registration, mut live) = state.hub.register(session_id);
        let mut conn = Connection {
            state: Arc::clone(&state),
            session_id,
            sink,
            sent: HashSet::new(),
            floor: established.replay_after,
        };
        let welcome = match session::welcome(&state, &established).await {
            Ok(w) => w,
            Err(e) => {
                tracing::error!(error = %e, %session_id, "no se pudo preparar welcome");
                close(&mut conn.sink, CLOSE_UNAVAILABLE, "servicio no disponible").await;
                return;
            }
        };
        if send_control(&mut conn.sink, ServerControl::Welcome(welcome))
            .await
            .is_err()
        {
            return;
        }
        // 3. reenviar lo que falte
        if conn.catch_up().await.is_err() {
            return;
        }
        state.records.record(
            RecordEvent::new(EventKind::ConnectionOpened, producer(&state))
                .session(session_id)
                .attr("transport", "websocket")
                .attr("resumed", established.resumed)
                .attr("client_last_seq", established.replay_after),
        );

        // 4. bucle principal
        let reason = conn.main_loop(&mut stream, &mut live).await;
        state.records.record(
            RecordEvent::new(EventKind::ConnectionClosed, producer(&state))
                .session(session_id)
                .attr("transport", "websocket")
                .attr("reason", reason)
                .attr(
                    "duration_ms",
                    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                ),
        );
    }

    async fn main_loop(
        &mut self,
        stream: &mut futures::stream::SplitStream<WebSocket>,
        live: &mut tokio::sync::mpsc::Receiver<LiveFrame>,
    ) -> &'static str {
        let mut last_activity = Instant::now();
        let mut ping = tokio::time::interval(SERVER_PING_EVERY);
        let mut catch_up = tokio::time::interval(CATCH_UP_EVERY);
        ping.reset();
        catch_up.reset();
        loop {
            tokio::select! {
                incoming = stream.next() => match incoming {
                    Some(Ok(Message::Text(text))) => {
                        last_activity = Instant::now();
                        match session::handle_command(&self.state, self.session_id, &text).await {
                            CommandReply::Reply(reply) => {
                                if send_control(&mut self.sink, reply).await.is_err() { return "send_failed"; }
                            }
                            CommandReply::Nothing => {}
                            CommandReply::SessionGone(reply) => {
                                // La sesión expiró con la conexión abierta: el
                                // cliente hace hello de nuevo y reenvía lo pendiente.
                                let _ = send_control(&mut self.sink, reply).await;
                                close(&mut self.sink, CLOSE_SESSION_GONE, "sesión expirada").await;
                                return "session_gone";
                            }
                        }
                    }
                    Some(Ok(Message::Binary(_))) => {
                        let err = error(ErrorCode::InvalidMessage, "solo se aceptan mensajes de texto".into(), None, false);
                        if send_control(&mut self.sink, err).await.is_err() { return "send_failed"; }
                    }
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => last_activity = Instant::now(),
                    Some(Ok(Message::Close(_))) => return "client_closed",
                    Some(Err(_)) | None => return "connection_lost",
                },
                Some(frame) = live.recv() => {
                    if self.send_frame(frame.seq, &frame.frame).await.is_err() {
                        return "send_failed";
                    }
                }
                _ = catch_up.tick() => {
                    if last_activity.elapsed() > IDLE_TIMEOUT {
                        close(&mut self.sink, CLOSE_IDLE, "sin actividad").await;
                        return "idle_timeout";
                    }
                    if self.catch_up().await.is_err() {
                        return "send_failed";
                    }
                }
                _ = ping.tick() => {
                    if self.sink.send(Message::Ping(Default::default())).await.is_err() {
                        return "send_failed";
                    }
                }
            }
        }
    }

    /// Envía un frame secuenciado si no se había enviado ya.
    async fn send_frame(&mut self, seq: u64, frame: &str) -> Result<(), axum::Error> {
        if seq <= self.floor || !self.sent.insert(seq) {
            return Ok(());
        }
        self.sink
            .send(Message::Text(Utf8Bytes::from(frame)))
            .await?;
        while self.sent.remove(&(self.floor + 1)) {
            self.floor += 1;
        }
        Ok(())
    }

    /// Envía todo lo que exista en Valkey por encima de `floor`.
    async fn catch_up(&mut self) -> Result<(), axum::Error> {
        let Ok(latest) = self.state.store.latest_seq(self.session_id).await else {
            // Si Valkey no responde, se intentará en la siguiente vuelta.
            return Ok(());
        };
        if latest <= self.floor {
            return Ok(());
        }
        let Ok(frames) = self.state.store.replay(self.session_id, self.floor).await else {
            return Ok(());
        };
        for (seq, frame) in frames {
            self.send_frame(seq, &frame).await?;
        }
        Ok(())
    }
}

async fn send_control(sink: &mut Sink, message: ServerControl) -> Result<(), axum::Error> {
    let json = serde_json::to_string(&message).map_err(axum::Error::new)?;
    sink.send(Message::Text(Utf8Bytes::from(json))).await
}

async fn close(sink: &mut Sink, code: u16, reason: &'static str) {
    let _ = sink
        .send(Message::Close(Some(CloseFrame {
            code,
            reason: Utf8Bytes::from_static(reason),
        })))
        .await;
}
