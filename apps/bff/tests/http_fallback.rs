#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Transporte HTTP de respaldo: mismo protocolo, mismas garantías.

mod common;

use std::{sync::Arc, time::Duration};

use bff::c5::{
    C5InboundEvent, C5InboundKind,
    memory::{Call, InMemoryGateway},
};
use common::*;
use protocol::Timestamp;
use serde_json::{Value, json};
use uuid::Uuid;

fn hello_body() -> Value {
    json!({
        "type": "hello", "client_msg_id": Uuid::new_v4(), "protocol_version": 1,
        "app_version": "0.1.0", "language": "es-MX"
    })
}

async fn post(bff: &TestBff, path: &str, body: &Value, origin: Option<&str>) -> (u16, Value) {
    let mut request = reqwest::Client::new()
        .post(format!("http://{}{path}", bff.addr))
        .json(body);
    if let Some(origin) = origin {
        request = request.header("origin", origin);
    }
    let response = request.send().await.unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

#[tokio::test]
async fn full_conversation_over_http() {
    let (_valkey, url) = start_valkey().await;
    let gateway = Arc::new(InMemoryGateway::default());
    let bff = TestBff::start(&url, "bff-a", Arc::clone(&gateway)).await;

    let (status, hello) = post(&bff, "/api/v1/hello", &hello_body(), None).await;
    assert_eq!(status, 200);
    let welcome = &hello["welcome"];
    assert_eq!(welcome["type"], "welcome");
    let (sid, token) = (
        welcome["session_id"].clone(),
        welcome["session_token"].clone(),
    );

    let chat_id = Uuid::new_v4();
    let sync = json!({
        "session_id": sid, "session_token": token, "last_seq": 0, "wait_ms": 0,
        "commands": [{ "type": "chat_message", "client_msg_id": chat_id, "kind": "text", "text": "ayuda por HTTP" }]
    });
    let (status, response) = post(&bff, "/api/v1/sync", &sync, None).await;
    assert_eq!(status, 200);
    assert_eq!(response["replies"][0]["type"], "ack");
    assert_eq!(response["replies"][0]["status"], "accepted");
    assert_eq!(response["frames"][0]["type"], "chat_message");

    // Reenviar el mismo comando (respuesta perdida): no se duplica.
    let (_, again) = post(&bff, "/api/v1/sync", &sync, None).await;
    assert_eq!(again["replies"][0]["status"], "duplicate");

    // Long-poll: espera hasta que el operador responde.
    let last = again["latest_seq"].as_u64().unwrap();
    let waiting = {
        let body = json!({ "session_id": sid, "session_token": token, "last_seq": last, "wait_ms": 10_000 });
        let addr = bff.addr;
        tokio::spawn(async move {
            let response = reqwest::Client::new()
                .post(format!("http://{addr}/api/v1/sync"))
                .json(&body)
                .send()
                .await
                .unwrap();
            response.json::<Value>().await.unwrap()
        })
    };
    let c5 = loop {
        if gateway
            .calls()
            .iter()
            .any(|c| matches!(c, Call::CreateIncident(_)))
        {
            break "C5INC-000001";
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    tokio::time::sleep(Duration::from_millis(300)).await;
    gateway.push_event(C5InboundEvent {
        event_id: "EVT-H".into(),
        c5_incident_id: c5.into(),
        occurred_at: Timestamp::now(),
        kind: C5InboundKind::OperatorMessage {
            c5_message_id: "M".into(),
            text: "te leemos".into(),
        },
    });
    let woke = tokio::time::timeout(Duration::from_secs(8), waiting)
        .await
        .unwrap()
        .unwrap();
    let frames = woke["frames"].as_array().unwrap();
    assert!(
        frames
            .iter()
            .any(|f| f["author"] == "operator" && f["text"] == "te leemos"),
        "{woke}"
    );
    assert!(frames.iter().all(|f| f["seq"].as_u64().unwrap() > last));
}

#[tokio::test]
async fn websocket_and_http_share_idempotency_and_seq() {
    let (_valkey, url) = start_valkey().await;
    let gateway = Arc::new(InMemoryGateway::default());
    let bff = TestBff::start(&url, "bff-a", Arc::clone(&gateway)).await;

    // Empieza por WebSocket…
    let mut ws = Client::connect(bff.addr).await;
    let welcome = ws.hello(json!({})).await;
    let id = Uuid::new_v4();
    let message =
        json!({ "type": "chat_message", "client_msg_id": id, "kind": "text", "text": "uno" });
    ws.send(message.clone()).await;
    ws.recv_control().await;
    let seen = ws.last_seq;
    ws.close().await;

    // …y la red bloquea WebSocket: sigue por HTTP con la misma sesión.
    let resume = json!({
        "type": "hello", "client_msg_id": Uuid::new_v4(), "protocol_version": 1, "app_version": "0.1.0",
        "language": "es-MX",
        "resume": { "session_id": welcome.session_id, "session_token": welcome.session_token, "last_seq": seen }
    });
    let (_, hello) = post(&bff, "/api/v1/hello", &resume, None).await;
    assert_eq!(hello["welcome"]["resumed"], true);
    let sync = json!({
        "session_id": welcome.session_id, "session_token": hello["welcome"]["session_token"],
        "last_seq": seen, "commands": [message]
    });
    let (_, response) = post(&bff, "/api/v1/sync", &sync, None).await;
    assert_eq!(
        response["replies"][0]["status"], "duplicate",
        "el reenvío por HTTP no duplica"
    );
    tokio::time::sleep(Duration::from_millis(500)).await;
    let sends = gateway
        .calls()
        .iter()
        .filter(|c| matches!(c, Call::SendMessage(_)))
        .count();
    assert_eq!(sends, 1);
}

#[tokio::test]
async fn rejects_bad_token_foreign_origin_and_old_versions() {
    let (_valkey, url) = start_valkey().await;
    let bff = TestBff::start(&url, "bff-a", Arc::new(InMemoryGateway::default())).await;
    let (_, hello) = post(&bff, "/api/v1/hello", &hello_body(), None).await;
    let sid = hello["welcome"]["session_id"].clone();

    let bad = json!({ "session_id": sid, "session_token": "token-falso", "last_seq": 0 });
    let (status, body) = post(&bff, "/api/v1/sync", &bad, None).await;
    assert_eq!(status, 401);
    assert_eq!(body["code"], "hello_required");

    let (status, _) = post(
        &bff,
        "/api/v1/hello",
        &hello_body(),
        Some("https://evil.example"),
    )
    .await;
    assert_eq!(status, 403);

    let mut old = hello_body();
    old["protocol_version"] = json!(99);
    let (status, body) = post(&bff, "/api/v1/hello", &old, None).await;
    assert_eq!(status, 426);
    assert_eq!(body["supported_versions"], json!([1]));
}
