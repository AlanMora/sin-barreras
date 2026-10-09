#![allow(clippy::unwrap_used, clippy::expect_used)]
//! El simulador se comporta como un operador y su feed es reanudable.

use std::time::Duration;

use c5_mock::config::{Chaos, Config};
use futures::StreamExt;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};

async fn start(api_key: &str, failure_rate: f64) -> std::net::SocketAddr {
    let config = Config {
        listen_addr: "127.0.0.1:0".parse().unwrap(),
        api_key: api_key.to_owned(),
        chaos: Chaos {
            latency_min_ms: 0,
            latency_max_ms: 5,
            failure_rate,
            feed_drop_every_secs: 0,
        },
        auto_operator: true,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, c5_mock::app(config)).await.unwrap() });
    addr
}

async fn post(addr: std::net::SocketAddr, path: &str, body: Value, key: &str) -> (u16, Value) {
    // Cliente HTTP mínimo sin dependencias extra.
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let payload = body.to_string();
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nx-api-key: {key}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    let status = response[9..12].parse().unwrap();
    let body = response.split("\r\n\r\n").nth(1).unwrap_or_default();
    (status, serde_json::from_str(body).unwrap_or(Value::Null))
}

async fn feed(addr: std::net::SocketAddr, since: u64, key: &str) -> impl StreamExt<Item = Value> {
    let mut request = format!("ws://{addr}/mock-api/v1/events?since={since}")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("x-api-key", key.parse().unwrap());
    let (ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    ws.filter_map(|m| async move {
        match m {
            Ok(Message::Text(t)) => serde_json::from_str(&t).ok(),
            _ => None,
        }
    })
}

#[tokio::test]
async fn operator_attends_replies_and_feed_is_resumable() {
    let addr = start("k", 0.0).await;
    let key = uuid::Uuid::new_v4();
    let body = json!({ "idempotency_key": key, "external_ref": uuid::Uuid::new_v4(), "channel": "text_chat", "language": "es-MX" });
    let (status, created) = post(addr, "/mock-api/v1/incidents", body.clone(), "k").await;
    assert_eq!(status, 201);
    let incident = created["incident_id"].as_str().unwrap().to_owned();
    // Idempotente: misma llave → mismo incidente.
    let (_, again) = post(addr, "/mock-api/v1/incidents", body, "k").await;
    assert_eq!(again["incident_id"], incident.as_str());

    let mut events = Box::pin(feed(addr, 0, "k").await);
    let attending = tokio::time::timeout(Duration::from_secs(5), events.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(attending["type"], "incident_status");
    assert_eq!(attending["status"], "attending");
    let greeting = tokio::time::timeout(Duration::from_secs(5), events.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(greeting["type"], "operator_message");

    let msg_key = uuid::Uuid::new_v4();
    let (status, _) = post(
        addr,
        &format!("/mock-api/v1/incidents/{incident}/messages"),
        json!({ "idempotency_key": msg_key, "kind": "quick_reply", "text": "Estoy en peligro", "quick_reply": "in_danger" }),
        "k",
    )
    .await;
    assert_eq!(status, 201);
    let read = tokio::time::timeout(Duration::from_secs(5), events.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read["type"], "message_status");
    assert_eq!(read["status"], "read");
    assert_eq!(read["idempotency_key"], msg_key.to_string());

    // Un feed nuevo desde el evento 2 recibe solo lo posterior.
    let mut resumed = Box::pin(feed(addr, 2, "k").await);
    let first = tokio::time::timeout(Duration::from_secs(5), resumed.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first["event_seq"], 3);
}

#[tokio::test]
async fn rejects_wrong_api_key_and_unknown_incident() {
    let addr = start("secreta", 0.0).await;
    let (status, _) = post(addr, "/mock-api/v1/incidents", json!({}), "otra").await;
    assert_eq!(status, 401);
    let (status, _) = post(
        addr,
        "/mock-api/v1/incidents/NO-EXISTE/messages",
        json!({ "idempotency_key": uuid::Uuid::new_v4(), "kind": "text", "text": "x" }),
        "secreta",
    )
    .await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn chaos_failure_rate_returns_503() {
    let addr = start("k", 1.0).await;
    let body = json!({ "idempotency_key": uuid::Uuid::new_v4(), "external_ref": uuid::Uuid::new_v4(), "channel": "text_chat", "language": "es" });
    let (status, _) = post(addr, "/mock-api/v1/incidents", body, "k").await;
    assert_eq!(status, 503);
}
