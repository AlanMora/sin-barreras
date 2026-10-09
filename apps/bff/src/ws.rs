//! Conexión WebSocket con la PWA (protocolo v1, ver `docs/contrato-eventos.md`).
//!
//! Ciclo de vida de una conexión:
//! 1. El primer mensaje debe ser `hello` (10 s máximo). Se negocia versión.
//! 2. Se crea o reanuda la sesión y se responde `welcome` con un token nuevo.
//! 3. Se reenvían los mensajes secuenciados que el cliente no tiene.
//! 4. Bucle: comandos del cliente → aceptación durable → `ack`; mensajes
//!    del bus → cliente; verificación periódica de `seq` para recuperar lo
//!    que el pub/sub pudiera haber perdido.

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
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use events::{EventKind, RecordEvent, SensitivePayload};
use futures::{SinkExt, StreamExt, stream::SplitSink};
use protocol::{
    AckStatus, Author, ClientMessage, EntryVerification, ErrorCode, HEARTBEAT_INTERVAL_MS, Hello,
    IncidentState, IncidentSummary, MAX_CLIENT_FRAME_BYTES, PROTOCOL_VERSION, Pong, ServerAck,
    ServerChatMessage, ServerControl, ServerError, ServerEvent, Timestamp, Welcome,
};
use uuid::Uuid;

use crate::{
    AppState,
    forwarder::{self, ID_NAMESPACE, location_payload, producer},
    hub::LiveFrame,
    store::{AcceptOutcome, ResumeOutcome},
    token,
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
const CLOSE_UNAVAILABLE: u16 = 4503;

/// Datos de red del cliente. Sensibles: solo se guardan cifrados en el registro.
#[derive(Debug, Clone)]
struct NetworkInfo {
    ip: Option<String>,
    user_agent: Option<String>,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    if !origin_allowed(
        &state.config.allowed_origins,
        headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()),
    ) {
        return (StatusCode::FORBIDDEN, "origen no permitido").into_response();
    }
    let network = NetworkInfo {
        ip: Some(client_ip(&headers, peer)),
        user_agent: headers
            .get(header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(|ua| ua.chars().take(512).collect()),
    };
    ws.max_message_size(MAX_CLIENT_FRAME_BYTES)
        .max_frame_size(MAX_CLIENT_FRAME_BYTES)
        .on_upgrade(move |socket| async move {
            Connection::run(state, socket, network).await;
        })
}

/// Los navegadores siempre envían `Origin` en un WebSocket; se valida para
/// evitar secuestro de WebSocket entre sitios (CSWSH). Clientes que no son
/// navegador (pruebas, herramientas) no lo envían y se permiten.
fn origin_allowed(allowed: &[String], origin: Option<&str>) -> bool {
    match origin {
        None => true,
        Some(origin) => allowed
            .iter()
            .any(|a| a == "*" || a.eq_ignore_ascii_case(origin)),
    }
}

/// IP del cliente: el BFF solo es alcanzable a través de Traefik, que agrega
/// la IP real al final de `X-Forwarded-For`.
fn client_ip(headers: &HeaderMap, peer: SocketAddr) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit(',').next())
        .map(str::trim)
        .filter(|ip| ip.parse::<std::net::IpAddr>().is_ok())
        .map(str::to_owned)
        .unwrap_or_else(|| peer.ip().to_string())
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
            Ok(Some(Ok(Message::Text(text)))) => parse_hello(&text),
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
            let mut err = error(
                ErrorCode::UnsupportedProtocolVersion,
                format!("versión {} no soportada", hello.protocol_version),
                Some(hello.client_msg_id),
                false,
            );
            if let ServerControl::Error(e) = &mut err {
                e.supported_versions = Some(vec![PROTOCOL_VERSION]);
            }
            let _ = send_control(&mut sink, err).await;
            close(&mut sink, CLOSE_UNSUPPORTED_VERSION, "versión no soportada").await;
            return;
        }

        // 2. sesión
        let established = match establish_session(&state, &hello, &network).await {
            Ok(e) => e,
            Err(error) => {
                tracing::error!(%error, "no se pudo establecer la sesión");
                let _ = send_control(
                    &mut sink,
                    self::error(
                        ErrorCode::Internal,
                        "servicio no disponible".into(),
                        Some(hello.client_msg_id),
                        true,
                    ),
                )
                .await;
                close(&mut sink, CLOSE_UNAVAILABLE, "servicio no disponible").await;
                return;
            }
        };
        let session_id = established.session_id;
        if !dropped.is_empty() {
            state.recorder.record(
                RecordEvent::new(EventKind::FieldsDropped, producer(&state))
                    .session(session_id)
                    .attr("message_type", "hello")
                    .attr("fields", dropped.clone()),
            );
        }

        // Registrarse en el hub ANTES de leer el estado: nada se pierde entre
        // la lectura y la suscripción (los duplicados se descartan por `seq`).
        let (_registration, mut live) = state.hub.register(session_id);
        let mut conn = Connection {
            state: Arc::clone(&state),
            session_id,
            sink,
            sent: HashSet::new(),
            floor: 0,
        };

        let welcome = match conn.welcome(&hello, &established).await {
            Ok(w) => w,
            Err(error) => {
                tracing::error!(%error, %session_id, "no se pudo preparar welcome");
                close(&mut conn.sink, CLOSE_UNAVAILABLE, "servicio no disponible").await;
                return;
            }
        };
        let client_last_seq = if established.resumed {
            hello.resume.as_ref().map_or(0, |r| r.last_seq)
        } else {
            0
        };
        conn.floor = client_last_seq;
        if send_control(&mut conn.sink, welcome).await.is_err() {
            return;
        }
        // 3. reenviar lo que falte
        if conn.catch_up().await.is_err() {
            return;
        }
        state.recorder.record(
            RecordEvent::new(EventKind::ConnectionOpened, producer(&state))
                .session(session_id)
                .attr("resumed", established.resumed)
                .attr("client_last_seq", client_last_seq),
        );

        // 4. bucle principal
        let reason = conn.main_loop(&mut stream, &mut live).await;
        state.recorder.record(
            RecordEvent::new(EventKind::ConnectionClosed, producer(&state))
                .session(session_id)
                .attr("reason", reason)
                .attr(
                    "duration_ms",
                    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                ),
        );
    }

    async fn welcome(
        &mut self,
        hello: &Hello,
        est: &Established,
    ) -> Result<ServerControl, crate::valkey::ValkeyError> {
        let fields = self.state.store.session_fields(self.session_id).await?;
        let latest_seq = self.state.store.latest_seq(self.session_id).await?;
        let incident = match (
            fields.get("incident_id").and_then(|v| v.parse().ok()),
            fields
                .get("channel")
                .and_then(|c| serde_json::from_value(serde_json::Value::String(c.clone())).ok()),
        ) {
            (Some(incident_id), Some(channel)) => Some(IncidentSummary {
                incident_id,
                channel,
                status: fields
                    .get("incident_status")
                    .and_then(|s| serde_json::from_value(serde_json::Value::String(s.clone())).ok())
                    .unwrap_or(IncidentState::Received),
            }),
            _ => None,
        };
        let ttl_ms = fields
            .get("ttl_ms")
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);
        let now = Timestamp::now();
        let _ = hello;
        Ok(ServerControl::Welcome(Welcome {
            protocol_version: PROTOCOL_VERSION,
            session_id: self.session_id,
            session_token: est.token.clone(),
            token_expires_at: Timestamp::from_unix_millis(now.unix_millis() + ttl_ms)
                .unwrap_or(now),
            resumed: est.resumed,
            entry: est.entry,
            heartbeat_interval_ms: HEARTBEAT_INTERVAL_MS,
            latest_seq,
            incident,
            server_ts: now,
        }))
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
                        if let Err(reason) = self.on_text(&text).await {
                            return reason;
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
        let latest = match self.state.store.latest_seq(self.session_id).await {
            Ok(latest) => latest,
            // Si Valkey no responde, se intentará en la siguiente vuelta.
            Err(_) => return Ok(()),
        };
        if latest <= self.floor {
            return Ok(());
        }
        let frames = match self.state.store.replay(self.session_id, self.floor).await {
            Ok(frames) => frames,
            Err(_) => return Ok(()),
        };
        for (seq, frame) in frames {
            self.send_frame(seq, &frame).await?;
        }
        Ok(())
    }

    async fn on_text(&mut self, text: &str) -> Result<(), &'static str> {
        if text.len() > MAX_CLIENT_FRAME_BYTES {
            return self
                .reply(error(
                    ErrorCode::MessageTooLarge,
                    "mensaje demasiado grande".into(),
                    None,
                    false,
                ))
                .await;
        }
        let mut message: ClientMessage = match serde_json::from_str(text) {
            Ok(m) => m,
            Err(e) => {
                let client_msg_id = extract_client_msg_id(text);
                self.record_protocol_error("parse", format!("{:?}", e.classify()), client_msg_id);
                return self
                    .reply(error(
                        ErrorCode::InvalidMessage,
                        "mensaje no válido".into(),
                        client_msg_id,
                        false,
                    ))
                    .await;
            }
        };
        let dropped = message.sanitize();
        if !dropped.is_empty() {
            self.state.recorder.record(
                RecordEvent::new(EventKind::FieldsDropped, producer(&self.state))
                    .session(self.session_id)
                    .attr("message_type", message.kind())
                    .attr("client_msg_id", message.client_msg_id().to_string())
                    .attr("fields", dropped),
            );
        }
        let cmid = message.client_msg_id();
        if let Err(invalid) = message.validate() {
            self.record_protocol_error("validation", invalid.to_string(), Some(cmid));
            return self
                .reply(error(
                    ErrorCode::InvalidMessage,
                    invalid.to_string(),
                    Some(cmid),
                    false,
                ))
                .await;
        }

        match &message {
            ClientMessage::Hello(_) => {
                self.reply(error(
                    ErrorCode::InvalidMessage,
                    "hello duplicado".into(),
                    Some(cmid),
                    false,
                ))
                .await
            }
            ClientMessage::Ping(_) => {
                let _ = self.state.store.touch(self.session_id).await;
                self.reply(ServerControl::Pong(Pong {
                    client_msg_id: cmid,
                    server_ts: Timestamp::now(),
                }))
                .await
            }
            ClientMessage::Ack(ack) => {
                let _ = self
                    .state
                    .store
                    .set_fields(self.session_id, &[("last_ack_seq", ack.seq.to_string())])
                    .await;
                self.state.recorder.record(
                    RecordEvent::new(EventKind::ClientAck, producer(&self.state))
                        .session(self.session_id)
                        .client_ts(ack.client_ts)
                        .attr("seq", ack.seq),
                );
                Ok(())
            }
            _ => self.accept(message).await,
        }
    }

    /// Acepta un comando de forma durable y responde `ack`.
    async fn accept(&mut self, message: ClientMessage) -> Result<(), &'static str> {
        let cmid = message.client_msg_id();
        let now = Timestamp::now();
        let forward = !matches!(message, ClientMessage::ClientEvent(_));
        let echo = match &message {
            ClientMessage::ChatMessage(m) => Some(ServerEvent::ChatMessage(ServerChatMessage {
                message_id: Uuid::new_v5(&ID_NAMESPACE, cmid.as_bytes()),
                author: Author::Citizen,
                kind: m.kind,
                text: m.text.clone(),
                quick_reply: m.quick_reply,
                client_msg_id: Some(cmid),
                server_ts: now,
            })),
            _ => None,
        };
        let json = match serde_json::to_string(&message) {
            Ok(json) => json,
            Err(_) => {
                return self
                    .reply(error(
                        ErrorCode::Internal,
                        "error interno".into(),
                        Some(cmid),
                        true,
                    ))
                    .await;
            }
        };

        let outcome = self
            .state
            .store
            .accept(
                self.session_id,
                cmid,
                &json,
                now.unix_millis(),
                forward,
                echo.as_ref(),
            )
            .await;
        match outcome {
            Ok(AcceptOutcome::Accepted { echo_seq }) => {
                self.record_accepted(&message, echo_seq);
                if forward {
                    forwarder::kick(&self.state, self.session_id);
                }
                self.reply(ServerControl::Ack(ServerAck {
                    client_msg_id: cmid,
                    status: AckStatus::Accepted,
                    server_ts: now,
                }))
                .await
            }
            Ok(AcceptOutcome::Duplicate) => {
                self.reply(ServerControl::Ack(ServerAck {
                    client_msg_id: cmid,
                    status: AckStatus::Duplicate,
                    server_ts: now,
                }))
                .await
            }
            Ok(AcceptOutcome::SessionMissing) => {
                // La sesión expiró con la conexión abierta: el cliente debe
                // hacer hello de nuevo y reenviar lo que no tenga ack.
                let _ = self
                    .reply(error(
                        ErrorCode::Internal,
                        "la sesión expiró".into(),
                        Some(cmid),
                        true,
                    ))
                    .await;
                close(&mut self.sink, CLOSE_SESSION_GONE, "sesión expirada").await;
                Err("session_gone")
            }
            Err(e) => {
                // Sin ack: el cliente conserva el mensaje en su cola y lo reintenta.
                tracing::error!(error = %e, session_id = %self.session_id, "no se pudo aceptar un comando");
                self.reply(error(
                    ErrorCode::Internal,
                    "no se pudo guardar; reintenta".into(),
                    Some(cmid),
                    true,
                ))
                .await
            }
        }
    }

    fn record_accepted(&self, message: &ClientMessage, echo_seq: Option<u64>) {
        let state = &self.state;
        let cmid = message.client_msg_id().to_string();
        let event = match message {
            ClientMessage::CreateIncident(m) => {
                RecordEvent::new(EventKind::IncidentRequested, producer(state))
                    .client_ts(m.client_ts)
                    .trace(m.traceparent.as_deref())
                    .attr("channel", m.channel.as_str())
                    .attr("language", m.language.clone())
            }
            ClientMessage::ChatMessage(m) => {
                RecordEvent::new(EventKind::CitizenMessage, producer(state))
                    .client_ts(m.client_ts)
                    .trace(m.traceparent.as_deref())
                    .attr("kind", serde_json::to_value(m.kind).unwrap_or_default())
                    .attr(
                        "quick_reply",
                        serde_json::to_value(m.quick_reply).unwrap_or_default(),
                    )
                    .attr("seq", echo_seq)
                    .sensitive(SensitivePayload::Message {
                        text: m.text.clone(),
                    })
            }
            ClientMessage::LocationUpdate(m) => {
                RecordEvent::new(EventKind::LocationReceived, producer(state))
                    .client_ts(m.client_ts)
                    .trace(m.traceparent.as_deref())
                    .attr("source", serde_json::to_value(m.source).unwrap_or_default())
                    .sensitive(location_payload(m))
            }
            ClientMessage::VideoCallRequest(m) => {
                RecordEvent::new(EventKind::VideoCallRequested, producer(state))
                    .client_ts(m.client_ts)
                    .trace(m.traceparent.as_deref())
                    .attr("stage", "received")
            }
            ClientMessage::ClientEvent(m) => {
                RecordEvent::new(EventKind::ClientEvent, producer(state))
                    .client_ts(m.client_ts)
                    .trace(m.traceparent.as_deref())
                    .attr("event", serde_json::to_value(m.kind).unwrap_or_default())
                    .attr("detail", m.detail.clone())
            }
            ClientMessage::Close(m) => RecordEvent::new(EventKind::ClientEvent, producer(state))
                .client_ts(m.client_ts)
                .attr("event", "close_requested")
                .attr("detail", serde_json::to_value(m.reason).unwrap_or_default()),
            ClientMessage::Hello(_) | ClientMessage::Ack(_) | ClientMessage::Ping(_) => return,
        };
        state
            .recorder
            .record(event.session(self.session_id).attr("client_msg_id", cmid));
    }

    fn record_protocol_error(&self, stage: &str, reason: String, client_msg_id: Option<Uuid>) {
        self.state.recorder.record(
            RecordEvent::new(EventKind::ProtocolError, producer(&self.state))
                .session(self.session_id)
                .attr("stage", stage)
                .attr("reason", reason)
                .attr("client_msg_id", client_msg_id.map(|id| id.to_string())),
        );
    }

    async fn reply(&mut self, message: ServerControl) -> Result<(), &'static str> {
        send_control(&mut self.sink, message)
            .await
            .map_err(|_| "send_failed")
    }
}

struct Established {
    session_id: Uuid,
    token: String,
    resumed: bool,
    entry: EntryVerification,
}

async fn establish_session(
    state: &Arc<AppState>,
    hello: &Hello,
    network: &NetworkInfo,
) -> Result<Established, Box<dyn std::error::Error + Send + Sync>> {
    let token = token::generate()?;
    let token_hash = token::hash(&token);
    let now = Timestamp::now();

    if let Some(resume) = &hello.resume {
        let outcome = state
            .store
            .resume_session(
                resume.session_id,
                &token::hash(&resume.session_token),
                &token_hash,
            )
            .await?;
        if let ResumeOutcome::Resumed { latest_seq } = outcome {
            let entry = state
                .store
                .session_fields(resume.session_id)
                .await?
                .get("entry_status")
                .map(|s| match s.as_str() {
                    "verified" => EntryVerification::Verified,
                    "unverified" => EntryVerification::Unverified,
                    _ => EntryVerification::None,
                })
                .unwrap_or(EntryVerification::None);
            state.recorder.record(
                RecordEvent::new(EventKind::SessionResumed, producer(state))
                    .session(resume.session_id)
                    .client_ts(hello.client_ts)
                    .attr("client_last_seq", resume.last_seq)
                    .attr("latest_seq", latest_seq),
            );
            record_metadata(state, resume.session_id, hello, network);
            return Ok(Established {
                session_id: resume.session_id,
                token,
                resumed: true,
                entry,
            });
        }
        state.recorder.record(
            RecordEvent::new(EventKind::SessionResumeRejected, producer(state))
                .attr("requested_session_id", resume.session_id.to_string())
                .attr("reason", format!("{outcome:?}").to_lowercase()),
        );
    }

    let session_id = Uuid::now_v7();
    let entry = state.qr.verify(hello.entry.as_ref());
    let point_id = hello
        .entry
        .as_ref()
        .map(|e| e.point_id.clone())
        .unwrap_or_default();
    let fields = vec![
        ("entry_point", point_id.clone()),
        ("entry_status", entry.as_str().to_owned()),
        ("language", hello.language.clone()),
        ("app_version", hello.app_version.clone()),
        ("protocol_version", hello.protocol_version.to_string()),
    ];
    state
        .store
        .create_session(
            session_id,
            &token_hash,
            now.unix_millis(),
            state.config.sessions.idle_ttl,
            &fields,
        )
        .await?;
    state.recorder.record(
        RecordEvent::new(EventKind::SessionStarted, producer(state))
            .session(session_id)
            .client_ts(hello.client_ts),
    );
    state.recorder.record(
        RecordEvent::new(EventKind::EntryPoint, producer(state))
            .session(session_id)
            .attr("source", if hello.entry.is_some() { "qr" } else { "web" })
            .attr("point_id", point_id)
            .attr(
                "key_id",
                hello.entry.as_ref().and_then(|e| e.key_id.clone()),
            )
            .attr("verification", entry.as_str()),
    );
    record_metadata(state, session_id, hello, network);
    Ok(Established {
        session_id,
        token,
        resumed: false,
        entry,
    })
}

fn record_metadata(state: &Arc<AppState>, session_id: Uuid, hello: &Hello, network: &NetworkInfo) {
    state.recorder.record(
        RecordEvent::new(EventKind::ClientMetadata, producer(state))
            .session(session_id)
            .client_ts(hello.client_ts)
            .trace(hello.traceparent.as_deref())
            .attr("language", hello.language.clone())
            .attr("app_version", hello.app_version.clone())
            .attr("protocol_version", hello.protocol_version)
            .attr(
                "accessibility",
                serde_json::to_value(&hello.accessibility).unwrap_or_default(),
            )
            .sensitive(SensitivePayload::Network {
                ip: network.ip.clone(),
                user_agent: network.user_agent.clone(),
            }),
    );
}

type HelloError = (ErrorCode, String, Option<Uuid>);

fn parse_hello(text: &str) -> Result<(Hello, Vec<&'static str>), HelloError> {
    let mut message: ClientMessage = serde_json::from_str(text).map_err(|_| {
        (
            ErrorCode::InvalidMessage,
            "hello no válido".to_owned(),
            extract_client_msg_id(text),
        )
    })?;
    let dropped = message.sanitize();
    message.validate().map_err(|e| {
        (
            ErrorCode::InvalidMessage,
            e.to_string(),
            Some(message.client_msg_id()),
        )
    })?;
    match message {
        ClientMessage::Hello(hello) => Ok((hello, dropped)),
        other => Err((
            ErrorCode::HelloRequired,
            "el primer mensaje debe ser hello".into(),
            Some(other.client_msg_id()),
        )),
    }
}

/// Intenta recuperar el `client_msg_id` de un JSON inválido para que el
/// cliente sepa a qué mensaje corresponde el error.
fn extract_client_msg_id(text: &str) -> Option<Uuid> {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()?
        .get("client_msg_id")?
        .as_str()?
        .parse()
        .ok()
}

fn error(
    code: ErrorCode,
    message: String,
    client_msg_id: Option<Uuid>,
    retryable: bool,
) -> ServerControl {
    ServerControl::Error(ServerError {
        code,
        message,
        client_msg_id,
        retryable,
        supported_versions: None,
        server_ts: Timestamp::now(),
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_rules() {
        let allowed = vec!["https://sinbarreras.zapopan.gob.mx".to_owned()];
        assert!(origin_allowed(&allowed, None));
        assert!(origin_allowed(
            &allowed,
            Some("https://sinbarreras.zapopan.gob.mx")
        ));
        assert!(!origin_allowed(&allowed, Some("https://evil.example")));
        assert!(origin_allowed(
            &["*".to_owned()],
            Some("http://localhost:5173")
        ));
    }

    #[test]
    fn client_ip_prefers_rightmost_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.1.1.1, 203.0.113.9".parse().unwrap());
        let peer: SocketAddr = "10.0.0.5:4000".parse().unwrap();
        assert_eq!(client_ip(&headers, peer), "203.0.113.9");
        assert_eq!(client_ip(&HeaderMap::new(), peer), "10.0.0.5");
    }

    #[test]
    fn extracts_client_msg_id_from_invalid_message() {
        let id = Uuid::new_v4();
        let raw = format!(r#"{{"type":"desconocido","client_msg_id":"{id}"}}"#);
        assert_eq!(extract_client_msg_id(&raw), Some(id));
        assert_eq!(extract_client_msg_id("no json"), None);
    }
}
