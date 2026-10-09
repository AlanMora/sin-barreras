//! Pruebas de integración del BFF con Valkey real (contenedor) y el C5 en memoria.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::{sync::Arc, time::Duration};

use bff::c5::{
    C5InboundEvent, C5InboundKind,
    memory::{Call, InMemoryGateway},
};
use common::*;
use events::{EventKind, SensitivePayload};

use protocol::{
    AckStatus, Author, DeliveryStatus, EntryVerification, ErrorCode, IncidentState, ServerControl,
    ServerEvent, Timestamp,
};
use serde_json::json;

fn operator_message(c5_incident_id: &str, event_id: &str, text: &str) -> C5InboundEvent {
    C5InboundEvent {
        event_id: event_id.to_owned(),
        c5_incident_id: c5_incident_id.to_owned(),
        occurred_at: Timestamp::now(),
        kind: C5InboundKind::OperatorMessage {
            c5_message_id: format!("M-{event_id}"),
            text: text.to_owned(),
        },
    }
}

fn c5_incident_id(gateway: &InMemoryGateway) -> Option<String> {
    // El adaptador en memoria asigna C5INC-000001 al primer incidente.
    gateway
        .calls()
        .iter()
        .any(|c| matches!(c, Call::CreateIncident(_)))
        .then(|| "C5INC-000001".to_owned())
}

fn is_ack(control: &ServerControl, status: AckStatus) -> bool {
    matches!(control, ServerControl::Ack(a) if a.status == status)
}

#[tokio::test]
async fn full_flow_incident_chat_and_operator_reply() {
    let (_valkey, url) = start_valkey().await;
    let gateway = Arc::new(InMemoryGateway::default());
    let bff = TestBff::start(&url, "bff-a", Arc::clone(&gateway)).await;
    let mut client = Client::connect(bff.addr).await;

    let welcome = client.hello(json!({})).await;
    assert!(!welcome.resumed);
    assert_eq!(welcome.entry, EntryVerification::None);
    assert_eq!(welcome.latest_seq, 0);

    let (_, ack) = client.command(create_incident("text_chat")).await;
    assert!(is_ack(&ack, AckStatus::Accepted));
    let created = client
        .recv_event(|e| matches!(e, ServerEvent::IncidentCreated(_)))
        .await;
    assert_eq!(created.seq, 1);
    let status = client
        .recv_event(|e| matches!(e, ServerEvent::IncidentStatus(_)))
        .await;
    assert_eq!(status.seq, 2);

    let (chat_id, ack) = client.command(chat("Necesito ayuda, me siento mal")).await;
    assert!(is_ack(&ack, AckStatus::Accepted));
    let echo = client
        .recv_event(|e| matches!(e, ServerEvent::ChatMessage(_)))
        .await;
    let ServerEvent::ChatMessage(echo) = echo.event else {
        unreachable!()
    };
    assert_eq!(echo.author, Author::Citizen);
    assert_eq!(echo.client_msg_id, Some(chat_id));
    let delivered = client
        .recv_event(|e| matches!(e, ServerEvent::MessageStatus(_)))
        .await;
    let ServerEvent::MessageStatus(delivered) = delivered.event else {
        unreachable!()
    };
    assert_eq!(
        (delivered.client_msg_id, delivered.status),
        (chat_id, DeliveryStatus::Delivered)
    );

    let c5_id = c5_incident_id(&gateway).expect("incidente creado en el C5");
    gateway.push_event(operator_message(
        &c5_id,
        "EVT-1",
        "Te estamos atendiendo. ¿Dónde estás?",
    ));
    let reply = client
        .recv_event(|e| matches!(e, ServerEvent::ChatMessage(m) if m.author == Author::Operator))
        .await;
    let ServerEvent::ChatMessage(reply) = reply.event else {
        unreachable!()
    };
    assert_eq!(reply.text, "Te estamos atendiendo. ¿Dónde estás?");

    // Todo quedó en el stream del registro, con el contenido CIFRADO.
    let stream = wait_record(&url, |e| e.kind == EventKind::FirstOperatorResponse).await;
    let events = stream.events;
    let kinds: Vec<_> = events.iter().map(|e| e.kind).collect();
    for expected in [
        EventKind::SessionStarted,
        EventKind::ClientMetadata,
        EventKind::EntryPoint,
        EventKind::IncidentRequested,
        EventKind::IncidentCreated,
        EventKind::CitizenMessage,
        EventKind::CitizenMessageForwarded,
        EventKind::OperatorMessage,
        EventKind::FirstOperatorResponse,
    ] {
        assert!(kinds.contains(&expected), "falta el evento {expected:?}");
    }
    let citizen = events
        .iter()
        .find(|e| e.kind == EventKind::CitizenMessage)
        .unwrap();
    assert_eq!(
        citizen.sensitive,
        Some(SensitivePayload::Message {
            text: "Necesito ayuda, me siento mal".into()
        })
    );
    assert!(
        !citizen
            .attributes
            .values()
            .any(|v| v.to_string().contains("Necesito ayuda"))
    );
    let created = events
        .iter()
        .find(|e| e.kind == EventKind::IncidentCreated)
        .unwrap();
    assert_eq!(created.c5_incident_id.as_deref(), Some(c5_id.as_str()));
    assert!(created.incident_id.is_some());
    let metadata = events
        .iter()
        .find(|e| e.kind == EventKind::ClientMetadata)
        .unwrap();
    assert!(matches!(
        &metadata.sensitive,
        Some(SensitivePayload::Network { ip: Some(_), .. })
    ));
    // El eco del mensaje y su registro dicen el mismo seq (lo asigna Valkey).
    assert_eq!(citizen.attributes["seq"], 3);
    let operator = events
        .iter()
        .find(|e| e.kind == EventKind::OperatorMessage)
        .unwrap();
    assert_eq!(
        operator.sensitive,
        Some(SensitivePayload::Message {
            text: "Te estamos atendiendo. ¿Dónde estás?".into()
        })
    );
    // En Valkey no hay contenido en claro: ni mensajes ni IP.
    for raw in &stream.raw {
        assert!(
            !raw.contains("Necesito ayuda")
                && !raw.contains("Dónde estás")
                && !raw.contains("127.0.0.1"),
            "{raw}"
        );
    }
    // El stream del registro NUNCA expira (sus eventos esperan a record-writer).
    let client = valkey_client::connect(&valkey_client::ValkeyConfig::plain(&url))
        .await
        .unwrap();
    let ttl: i64 = fred::interfaces::KeysInterface::pttl(&client, events::RECORD_STREAM)
        .await
        .unwrap();
    assert_eq!(ttl, -1);
}

#[tokio::test]
async fn each_record_is_written_exactly_once_even_with_duplicates_and_two_replicas() {
    let (_valkey, url) = start_valkey().await;
    let gateway = Arc::new(InMemoryGateway::default());
    let a = TestBff::start(&url, "bff-a", Arc::clone(&gateway)).await;
    let _b = TestBff::start(&url, "bff-b", Arc::clone(&gateway)).await;
    let mut client = Client::connect(a.addr).await;
    client.hello(json!({})).await;

    let id = uuid::Uuid::new_v4();
    let message =
        json!({ "type": "chat_message", "client_msg_id": id, "kind": "text", "text": "ayuda" });
    client.send(message.clone()).await;
    client.recv_control().await;
    client.send(message).await; // reenvío por reconexión
    client.recv_control().await;
    let c5_id = loop {
        if let Some(id) = c5_incident_id(&gateway) {
            break id;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    // Las dos réplicas reciben el mismo evento del C5.
    gateway.push_event(operator_message(&c5_id, "EVT-DUP", "recibido"));
    wait_record(&url, |e| e.kind == EventKind::OperatorMessage).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let stream = read_records(&url).await.events;
    let count = |kind: EventKind| stream.iter().filter(|e| e.kind == kind).count();
    assert_eq!(
        count(EventKind::CitizenMessage),
        1,
        "el reenvío no se registra dos veces"
    );
    assert_eq!(
        count(EventKind::OperatorMessage),
        1,
        "el evento del C5 se registra una vez aunque lo vean 2 réplicas"
    );
    assert_eq!(count(EventKind::SessionKeyCreated), 1);
    assert_eq!(count(EventKind::IncidentCreated), 1);
}

#[tokio::test]
async fn resent_message_is_not_duplicated() {
    let (_valkey, url) = start_valkey().await;
    let gateway = Arc::new(InMemoryGateway::default());
    let bff = TestBff::start(&url, "bff-a", Arc::clone(&gateway)).await;
    let mut client = Client::connect(bff.addr).await;
    client.hello(json!({})).await;

    let id = uuid::Uuid::new_v4();
    let message =
        json!({ "type": "chat_message", "client_msg_id": id, "kind": "text", "text": "ayuda" });
    client.send(message.clone()).await;
    let first = client.recv_control().await;
    assert!(is_ack(&first, AckStatus::Accepted));
    client.send(message).await;
    let second = client.recv_control().await;
    assert!(is_ack(&second, AckStatus::Duplicate));

    assert!(
        eventually(Duration::from_secs(5), || {
            gateway
                .calls()
                .iter()
                .filter(|c| matches!(c, Call::SendMessage(_)))
                .count()
                == 1
        })
        .await
    );
    tokio::time::sleep(Duration::from_millis(500)).await;
    let sends = gateway
        .calls()
        .iter()
        .filter(|c| matches!(c, Call::SendMessage(_)))
        .count();
    assert_eq!(
        sends, 1,
        "el mensaje reenviado no debe llegar dos veces al C5"
    );
    // Un chat sin incidente crea el incidente automáticamente (nunca se bloquea la ayuda).
    assert_eq!(
        gateway
            .calls()
            .iter()
            .filter(|c| matches!(c, Call::CreateIncident(_)))
            .count(),
        1
    );
}

#[tokio::test]
async fn reconnect_recovers_missed_messages_by_seq() {
    let (_valkey, url) = start_valkey().await;
    let gateway = Arc::new(InMemoryGateway::default());
    let bff = TestBff::start(&url, "bff-a", Arc::clone(&gateway)).await;

    let mut client = Client::connect(bff.addr).await;
    let welcome = client.hello(json!({})).await;
    client.command(create_incident("text_chat")).await;
    client
        .recv_event(|e| matches!(e, ServerEvent::IncidentStatus(_)))
        .await;
    let seen = client.last_seq;
    client.close().await;

    // Mientras el teléfono está bloqueado, el operador escribe dos veces.
    let c5_id = c5_incident_id(&gateway).unwrap();
    gateway.push_event(operator_message(&c5_id, "EVT-A", "uno"));
    gateway.push_event(operator_message(&c5_id, "EVT-B", "dos"));
    tokio::time::sleep(Duration::from_millis(300)).await;

    let mut client = Client::connect(bff.addr).await;
    let resumed = client.resume(&welcome, seen).await;
    assert!(resumed.resumed);
    assert_eq!(resumed.session_id, welcome.session_id);
    assert_eq!(resumed.latest_seq, seen + 2);
    assert_ne!(
        resumed.session_token, welcome.session_token,
        "el token se rota"
    );
    let a = client
        .recv_event(|e| matches!(e, ServerEvent::ChatMessage(_)))
        .await;
    let b = client
        .recv_event(|e| matches!(e, ServerEvent::ChatMessage(_)))
        .await;
    assert_eq!((a.seq, b.seq), (seen + 1, seen + 2));
    assert!(
        client.is_silent(Duration::from_millis(300)).await,
        "no debe reenviar lo que ya tenía"
    );

    // El token anterior ya no sirve: se crea una sesión nueva.
    let mut other = Client::connect(bff.addr).await;
    let rejected = other.resume(&welcome, 0).await;
    assert!(!rejected.resumed);
    assert_ne!(rejected.session_id, welcome.session_id);
}

#[tokio::test]
async fn operator_messages_reach_citizen_connected_to_another_replica_exactly_once() {
    let (_valkey, url) = start_valkey().await;
    let gateway = Arc::new(InMemoryGateway::default());
    let a = TestBff::start(&url, "bff-a", Arc::clone(&gateway)).await;
    let b = TestBff::start(&url, "bff-b", Arc::clone(&gateway)).await;

    let mut client = Client::connect(a.addr).await;
    client.hello(json!({})).await;
    client.command(create_incident("text_chat")).await;
    client
        .recv_event(|e| matches!(e, ServerEvent::IncidentStatus(_)))
        .await;

    // Ambas réplicas reciben el evento del C5; solo una debe emitirlo.
    let c5_id = c5_incident_id(&gateway).unwrap();
    gateway.push_event(operator_message(&c5_id, "EVT-X", "hola"));
    let reply = client
        .recv_event(|e| matches!(e, ServerEvent::ChatMessage(_)))
        .await;
    assert_eq!(reply.seq, 3);
    assert!(
        client.is_silent(Duration::from_millis(700)).await,
        "el mensaje no debe duplicarse"
    );
    drop(b);
}

#[tokio::test]
async fn pending_message_survives_replica_crash() {
    let (_valkey, url) = start_valkey().await;
    let gateway_a = Arc::new(InMemoryGateway::default());
    let gateway_b = Arc::new(InMemoryGateway::default());
    // El C5 no responde desde la réplica A.
    gateway_a.fail_transiently(u32::MAX);
    let a = TestBff::start(&url, "bff-a", Arc::clone(&gateway_a)).await;

    let mut client = Client::connect(a.addr).await;
    let welcome = client.hello(json!({})).await;
    let (chat_id, ack) = client.command(chat("estoy herido")).await;
    // El ack llega aunque el C5 no responda: el mensaje ya es durable en Valkey.
    assert!(is_ack(&ack, AckStatus::Accepted));
    let seen = client.last_seq;

    // La réplica A muere con el mensaje pendiente.
    a.kill();
    let b = TestBff::start(&url, "bff-b", Arc::clone(&gateway_b)).await;

    // B lo retoma (barrido + lease vencido) y lo entrega al C5.
    assert!(
        eventually(Duration::from_secs(15), || {
            gateway_b
                .calls()
                .iter()
                .any(|c| matches!(c, Call::SendMessage(m) if m.idempotency_key == chat_id))
        })
        .await,
        "la réplica B debe reenviar el mensaje pendiente"
    );

    // El ciudadano reconecta (a B) y recibe la confirmación de entrega.
    let mut client = Client::connect(b.addr).await;
    let resumed = client.resume(&welcome, seen).await;
    assert!(resumed.resumed);
    let status = client
        .recv_event(|e| matches!(e, ServerEvent::MessageStatus(s) if s.client_msg_id == chat_id))
        .await;
    let ServerEvent::MessageStatus(status) = status.event else {
        unreachable!()
    };
    assert_eq!(status.status, DeliveryStatus::Delivered);
}

#[tokio::test]
async fn transient_c5_failures_are_retried_in_order() {
    let (_valkey, url) = start_valkey().await;
    let gateway = Arc::new(InMemoryGateway::default());
    gateway.fail_transiently(3);
    let bff = TestBff::start(&url, "bff-a", Arc::clone(&gateway)).await;
    let mut client = Client::connect(bff.addr).await;
    client.hello(json!({})).await;

    let mut ids = Vec::new();
    for text in ["uno", "dos", "tres"] {
        let (id, _) = client.command(chat(text)).await;
        ids.push(id);
    }
    assert!(
        eventually(Duration::from_secs(10), || {
            gateway
                .calls()
                .iter()
                .filter(|c| matches!(c, Call::SendMessage(_)))
                .count()
                >= 3
        })
        .await
    );
    // Orden de llegada al C5 = orden en que el ciudadano los envió.
    let mut delivered: Vec<_> = Vec::new();
    for call in gateway.calls() {
        if let Call::SendMessage(m) = call
            && !delivered.contains(&m.idempotency_key)
        {
            delivered.push(m.idempotency_key);
        }
    }
    assert_eq!(delivered, ids);
}

#[tokio::test]
async fn c5_rejection_marks_message_as_failed() {
    let (_valkey, url) = start_valkey().await;
    let gateway = Arc::new(InMemoryGateway::default());
    let bff = TestBff::start(&url, "bff-a", Arc::clone(&gateway)).await;
    let mut client = Client::connect(bff.addr).await;
    client.hello(json!({})).await;
    client.command(create_incident("text_chat")).await;
    client
        .recv_event(|e| matches!(e, ServerEvent::IncidentStatus(_)))
        .await;

    gateway.reject(1);
    let (chat_id, _) = client.command(chat("hola")).await;
    let status = client
        .recv_event(|e| matches!(e, ServerEvent::MessageStatus(_)))
        .await;
    let ServerEvent::MessageStatus(status) = status.event else {
        unreachable!()
    };
    assert_eq!(
        (status.client_msg_id, status.status),
        (chat_id, DeliveryStatus::Failed)
    );
}

#[tokio::test]
async fn location_before_incident_is_sent_right_after_creation() {
    let (_valkey, url) = start_valkey().await;
    let gateway = Arc::new(InMemoryGateway::default());
    let bff = TestBff::start(&url, "bff-a", Arc::clone(&gateway)).await;
    let mut client = Client::connect(bff.addr).await;
    client.hello(json!({})).await;

    client
        .command(json!({
            "type": "location_update", "latitude": 20.7214, "longitude": -103.3918,
            "accuracy_m": 12.5, "source": "gps", "captured_at": "2026-10-09T18:00:00.000Z"
        }))
        .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        gateway.calls().is_empty(),
        "sin incidente no se envía nada al C5"
    );

    client.command(create_incident("silent")).await;
    assert!(
        eventually(Duration::from_secs(5), || {
            let calls = gateway.calls();
            matches!(calls.as_slice(), [Call::CreateIncident(_), Call::UpdateLocation(l)] if (l.latitude - 20.7214).abs() < 1e-9)
        })
        .await
    );
}

#[tokio::test]
async fn qr_entry_is_verified_and_invalid_qr_does_not_block() {
    let (_valkey, url) = start_valkey().await;
    let bff = TestBff::start(&url, "bff-a", Arc::new(InMemoryGateway::default())).await;
    let signature = bff::qr::QrVerifier::new([("1".to_owned(), QR_KEY.to_vec())].into())
        .sign("1", "TOT-0142")
        .unwrap();

    let mut client = Client::connect(bff.addr).await;
    let welcome = client
        .hello(
            json!({ "entry": { "point_id": "TOT-0142", "key_id": "1", "signature": signature } }),
        )
        .await;
    assert_eq!(welcome.entry, EntryVerification::Verified);

    let mut client = Client::connect(bff.addr).await;
    let welcome = client
        .hello(
            json!({ "entry": { "point_id": "TOT-9999", "key_id": "1", "signature": signature } }),
        )
        .await;
    assert_eq!(welcome.entry, EntryVerification::Unverified);
    let (_, ack) = client.command(create_incident("text_chat")).await;
    assert!(
        is_ack(&ack, AckStatus::Accepted),
        "un QR falso no impide pedir ayuda"
    );
}

#[tokio::test]
async fn protocol_errors_do_not_kill_the_connection() {
    let (_valkey, url) = start_valkey().await;
    let bff = TestBff::start(&url, "bff-a", Arc::new(InMemoryGateway::default())).await;
    let mut client = Client::connect(bff.addr).await;
    client.hello(json!({})).await;

    client.send_raw("esto no es json").await;
    assert!(
        matches!(client.recv_control().await, ServerControl::Error(e) if e.code == ErrorCode::InvalidMessage)
    );

    let (_, reply) = client
        .command(json!({ "type": "chat_message", "kind": "text", "text": "   " }))
        .await;
    assert!(
        matches!(reply, ServerControl::Error(e) if e.code == ErrorCode::InvalidMessage && !e.retryable)
    );

    client
        .send(json!({ "type": "ping", "client_msg_id": uuid::Uuid::new_v4() }))
        .await;
    assert!(matches!(
        client.recv_control().await,
        ServerControl::Pong(_)
    ));
}

#[tokio::test]
async fn unsupported_protocol_version_is_rejected_with_supported_list() {
    let (_valkey, url) = start_valkey().await;
    let bff = TestBff::start(&url, "bff-a", Arc::new(InMemoryGateway::default())).await;
    let mut client = Client::connect(bff.addr).await;
    client
        .send(json!({
            "type": "hello", "client_msg_id": uuid::Uuid::new_v4(), "protocol_version": 99,
            "app_version": "9.0.0", "language": "es-MX"
        }))
        .await;
    match client.recv_control().await {
        ServerControl::Error(e) => {
            assert_eq!(e.code, ErrorCode::UnsupportedProtocolVersion);
            assert_eq!(e.supported_versions, Some(vec![1]));
        }
        other => panic!("se esperaba error, llegó {other:?}"),
    }
}

#[tokio::test]
async fn incident_status_and_video_call_updates_reach_the_citizen() {
    let (_valkey, url) = start_valkey().await;
    let gateway = Arc::new(InMemoryGateway::default());
    let bff = TestBff::start(&url, "bff-a", Arc::clone(&gateway)).await;
    let mut client = Client::connect(bff.addr).await;
    client.hello(json!({})).await;
    client.command(create_incident("lsm_video")).await;
    client
        .recv_event(|e| matches!(e, ServerEvent::IncidentStatus(_)))
        .await;
    let c5_id = c5_incident_id(&gateway).unwrap();

    let (video_id, _) = client
        .command(json!({ "type": "video_call_request" }))
        .await;
    client
        .recv_event(|e| matches!(e, ServerEvent::VideoCall(v) if v.status == protocol::VideoCallState::Requested))
        .await;
    gateway.push_event(C5InboundEvent {
        event_id: "EVT-V".into(),
        c5_incident_id: c5_id.clone(),
        occurred_at: Timestamp::now(),
        kind: C5InboundKind::VideoCall {
            idempotency_key: video_id,
            status: protocol::VideoCallState::Ready,
            join: Some(json!({ "room": "sala-1", "token": "t" })),
        },
    });
    let ready = client
        .recv_event(|e| matches!(e, ServerEvent::VideoCall(v) if v.status == protocol::VideoCallState::Ready))
        .await;
    let ServerEvent::VideoCall(ready) = ready.event else {
        unreachable!()
    };
    assert_eq!(ready.join.unwrap()["room"], "sala-1");

    gateway.push_event(C5InboundEvent {
        event_id: "EVT-S".into(),
        c5_incident_id: c5_id,
        occurred_at: Timestamp::now(),
        kind: C5InboundKind::IncidentStatus {
            status: IncidentState::UnitEnRoute,
            eta_seconds: Some(480),
        },
    });
    let status = client
        .recv_event(|e| matches!(e, ServerEvent::IncidentStatus(s) if s.status == IncidentState::UnitEnRoute))
        .await;
    let ServerEvent::IncidentStatus(status) = status.event else {
        unreachable!()
    };
    assert_eq!(status.eta_seconds, Some(480));
}

/// Regresión: si el primer mensaje es un chat, el incidente se crea con una
/// llave de idempotencia PROPIA, distinta de la del chat. Con la misma llave,
/// el C5 devolvía la respuesta del incidente al enviar el mensaje y la cola de
/// la sesión quedaba bloqueada.
#[tokio::test]
async fn chat_as_first_message_uses_distinct_idempotency_keys() {
    let (_valkey, url) = start_valkey().await;
    let gateway = Arc::new(InMemoryGateway::default());
    let bff = TestBff::start(&url, "bff-a", Arc::clone(&gateway)).await;
    let mut client = Client::connect(bff.addr).await;
    client.hello(json!({})).await;
    let (chat_id, _) = client.command(chat("primer mensaje")).await;
    let status = client
        .recv_event(|e| matches!(e, ServerEvent::MessageStatus(_)))
        .await;
    let ServerEvent::MessageStatus(status) = status.event else {
        unreachable!()
    };
    assert_eq!(
        (status.client_msg_id, status.status),
        (chat_id, DeliveryStatus::Delivered)
    );
    let calls = gateway.calls();
    let create_key = calls.iter().find_map(|c| match c {
        Call::CreateIncident(r) => Some(r.idempotency_key),
        _ => None,
    });
    assert_eq!(
        create_key,
        Some(bff::forwarder::incident_idempotency_key(chat_id))
    );
    assert_ne!(create_key, Some(chat_id));
}

/// Rate limiting (ADR-0020): el exceso se pospone (reintentable), pedir ayuda nunca.
#[tokio::test]
async fn rate_limit_postpones_excess_but_never_blocks_asking_for_help() {
    let (_valkey, url) = start_valkey().await;
    let bff = TestBff::start(&url, "bff-a", Arc::new(InMemoryGateway::default())).await;
    let mut client = Client::connect(bff.addr).await;
    client.hello(json!({})).await;

    // El pedido de ayuda nunca se limita.
    let (_, ack) = client.command(create_incident("text_chat")).await;
    assert!(is_ack(&ack, AckStatus::Accepted));
    let mut accepted = 0;
    let mut limited = 0;
    for i in 0..20 {
        match client.command(chat(&format!("mensaje {i}"))).await.1 {
            ServerControl::Ack(_) => accepted += 1,
            ServerControl::Error(e) => {
                assert_eq!(e.code, ErrorCode::RateLimited);
                assert!(e.retryable, "el exceso se reintenta, no se pierde");
                limited += 1;
            }
            other => panic!("inesperado: {other:?}"),
        }
    }
    // 3 mensajes libres + 10 por minuto.
    assert_eq!(accepted, 13);
    assert_eq!(limited, 7);
    // Aun excedido el límite, otro create_incident (idempotente) se acepta.
    let (_, ack) = client.command(create_incident("text_chat")).await;
    assert!(is_ack(&ack, AckStatus::Accepted));
}
