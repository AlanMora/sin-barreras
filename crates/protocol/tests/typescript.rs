//! Genera `apps/pwa/src/protocol/generated.ts` a partir de los tipos Rust.
//!
//! - `cargo test -p protocol` falla si el archivo está desactualizado.
//! - `UPDATE_TS_BINDINGS=1 cargo test -p protocol` lo regenera.

use std::path::PathBuf;

use protocol::*;
use ts_rs::{Config, TS};

macro_rules! declarations {
    ($cfg:expr; $($ty:ty),* $(,)?) => {
        vec![$(format!("export {}", <$ty as TS>::decl($cfg))),*]
    };
}

fn render() -> String {
    // `seq` y los contadores caben de sobra en un `number` de JavaScript.
    let cfg = Config::new().with_large_int("number");
    let decls = declarations![&cfg;
        // Cliente → servidor
        ClientMessage, Hello, Resume, QrEntry, AccessibilitySettings, Channel, CreateIncident,
        ChatKind, QuickReply, ClientChatMessage, LocationSource, LocationUpdate, VideoCallRequest,
        ClientEventKind, ClientEvent, ClientAck, Ping, ClientCloseReason, ClientClose,
        // Servidor → cliente: control
        ServerControl, EntryVerification, Welcome, IncidentSummary, AckStatus, ServerAck, Pong,
        ErrorCode, ServerError,
        // Servidor → cliente: secuenciados
        SequencedMessage, ServerEvent, IncidentCreated, Author, ServerChatMessage, DeliveryStatus,
        MessageStatus, IncidentState, IncidentStatus, VideoCallState, VideoCallUpdate,
        SessionClosedReason, SessionClosed, Timestamp,
    ];
    let mut out = String::from(
        "// ARCHIVO GENERADO desde crates/protocol. NO EDITAR A MANO.\n\
         // Regenerar: UPDATE_TS_BINDINGS=1 cargo test -p protocol --test typescript\n\n",
    );
    out.push_str(&format!(
        "export const PROTOCOL_VERSION = {PROTOCOL_VERSION};\n"
    ));
    out.push_str(&format!(
        "export const MAX_CHAT_TEXT_CHARS = {MAX_CHAT_TEXT_CHARS};\n"
    ));
    out.push_str(&format!(
        "export const MAX_CLIENT_FRAME_BYTES = {MAX_CLIENT_FRAME_BYTES};\n"
    ));
    out.push_str(&format!(
        "export const HEARTBEAT_INTERVAL_MS = {HEARTBEAT_INTERVAL_MS};\n\n"
    ));
    out.push_str("/** Cualquier frame que envía el servidor. */\n");
    out.push_str("export type ServerFrame = ServerControl | SequencedMessage;\n\n");
    for decl in decls {
        out.push_str(&decl);
        out.push_str("\n\n");
    }
    out.trim_end().to_owned() + "\n"
}

#[test]
fn typescript_bindings_are_up_to_date() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/pwa/src/protocol/generated.ts");
    let expected = render();
    if std::env::var_os("UPDATE_TS_BINDINGS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &expected).unwrap();
        return;
    }
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        current == expected,
        "generated.ts está desactualizado. Ejecuta: UPDATE_TS_BINDINGS=1 cargo test -p protocol --test typescript"
    );
}
