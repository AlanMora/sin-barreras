//! Pruebas del formato JSON del contrato: lo que la PWA envía y recibe.

use protocol::*;
use serde_json::json;
use uuid::Uuid;

fn parse(value: serde_json::Value) -> Result<ClientMessage, serde_json::Error> {
    serde_json::from_value(value)
}

#[test]
fn parses_hello_with_resume_and_qr() {
    let msg = parse(json!({
        "type": "hello",
        "client_msg_id": "0b6c6f8e-8f2e-4c1a-9a51-2f3c2d1e0f00",
        "protocol_version": 1,
        "app_version": "0.1.0",
        "language": "es-MX",
        "resume": { "session_id": "3f0e1c2a-1111-4c1a-9a51-2f3c2d1e0f00", "session_token": "abc_DEF-123", "last_seq": 41 },
        "entry": { "point_id": "TOT-0142", "key_id": "1", "signature": "c2lnbmF0dXJl" }
    }))
    .unwrap();
    let ClientMessage::Hello(hello) = msg else {
        panic!("se esperaba hello")
    };
    assert_eq!(hello.resume.unwrap().last_seq, 41);
    assert_eq!(hello.entry.unwrap().point_id, "TOT-0142");
}

#[test]
fn rejects_unknown_fields() {
    let result = parse(json!({
        "type": "ping",
        "client_msg_id": Uuid::new_v4(),
        "inyectado": true
    }));
    assert!(result.is_err());
}

#[test]
fn rejects_unknown_type() {
    assert!(parse(json!({ "type": "drop_database", "client_msg_id": Uuid::new_v4() })).is_err());
}

#[test]
fn rejects_missing_client_msg_id() {
    assert!(parse(json!({ "type": "ping" })).is_err());
}

#[test]
fn quick_reply_requires_code() {
    let msg = parse(json!({
        "type": "chat_message",
        "client_msg_id": Uuid::new_v4(),
        "kind": "quick_reply",
        "text": "Estoy en peligro"
    }))
    .unwrap();
    assert_eq!(msg.validate().unwrap_err().field, "quick_reply");
}

#[test]
fn chat_text_limits() {
    let make = |text: String| {
        parse(json!({ "type": "chat_message", "client_msg_id": Uuid::new_v4(), "kind": "text", "text": text }))
            .unwrap()
    };
    assert!(make("Necesito ayuda".into()).validate().is_ok());
    assert!(make("   ".into()).validate().is_err());
    assert!(
        make("a".repeat(MAX_CHAT_TEXT_CHARS + 1))
            .validate()
            .is_err()
    );
    assert!(make("hola\u{0007}".into()).validate().is_err());
    assert!(make("línea 1\nlínea 2 ñ 🙏".into()).validate().is_ok());
}

#[test]
fn location_ranges() {
    let make = |lat: f64, lon: f64| {
        parse(json!({
            "type": "location_update", "client_msg_id": Uuid::new_v4(),
            "latitude": lat, "longitude": lon, "source": "gps",
            "captured_at": "2026-10-09T18:00:00.000Z"
        }))
        .unwrap()
    };
    assert!(make(20.72, -103.39).validate().is_ok());
    assert!(make(91.0, 0.0).validate().is_err());
    assert!(make(0.0, -181.0).validate().is_err());
}

#[test]
fn sanitize_drops_invalid_optional_fields_but_keeps_message() {
    let mut msg = parse(json!({
        "type": "hello",
        "client_msg_id": Uuid::new_v4(),
        "traceparent": "no-es-una-traza",
        "protocol_version": 1,
        "app_version": "0.1.0",
        "language": "español!!",
        "entry": { "point_id": "<script>", "signature": "x" },
        "accessibility": { "font_scale": 99.0, "high_contrast": true, "reduced_motion": false, "silent_mode": false }
    }))
    .unwrap();
    let dropped = msg.sanitize();
    assert_eq!(
        dropped,
        vec!["traceparent", "language", "entry", "accessibility"]
    );
    assert!(msg.validate().is_ok());
    let ClientMessage::Hello(hello) = msg else {
        panic!()
    };
    assert_eq!(hello.language, "es-MX");
    assert!(hello.entry.is_none());
}

#[test]
fn sequenced_message_json_shape() {
    let msg = SequencedMessage {
        seq: 7,
        event: ServerEvent::IncidentStatus(IncidentStatus {
            status: IncidentState::UnitEnRoute,
            eta_seconds: Some(480),
            server_ts: Timestamp::from_unix_millis(1_791_000_000_000).unwrap(),
        }),
    };
    let value = serde_json::to_value(&msg).unwrap();
    assert_eq!(value["seq"], 7);
    assert_eq!(value["type"], "incident_status");
    assert_eq!(value["status"], "unit_en_route");
    assert_eq!(value["eta_seconds"], 480);
}

#[test]
fn server_frame_distinguishes_control_and_sequenced() {
    let control: ServerFrame = serde_json::from_value(json!({
        "type": "pong", "client_msg_id": Uuid::new_v4(), "server_ts": "2026-10-09T18:00:00.000Z"
    }))
    .unwrap();
    assert!(matches!(
        control,
        ServerFrame::Control(ServerControl::Pong(_))
    ));

    let sequenced: ServerFrame = serde_json::from_value(json!({
        "seq": 3, "type": "session_closed", "reason": "incident_closed", "server_ts": "2026-10-09T18:00:00.000Z"
    }))
    .unwrap();
    assert!(matches!(
        sequenced,
        ServerFrame::Sequenced(SequencedMessage { seq: 3, .. })
    ));
}

/// La forma en que el BFF arma un mensaje secuenciado (anteponiendo `seq` al
/// JSON del evento, dentro de Valkey) debe producir un `SequencedMessage`.
#[test]
fn prefixing_seq_produces_valid_sequenced_message() {
    let event = ServerEvent::SessionClosed(SessionClosed {
        reason: SessionClosedReason::Expired,
        server_ts: Timestamp::now(),
    });
    let payload = serde_json::to_string(&event).unwrap();
    let framed = format!("{{\"seq\":12,{}", &payload[1..]);
    let parsed: SequencedMessage = serde_json::from_str(&framed).unwrap();
    assert_eq!(parsed.seq, 12);
    assert_eq!(parsed.event, event);
}
