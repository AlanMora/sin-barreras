//! Contrato de mensajes entre la PWA y el BFF (protocolo v1).
//!
//! Este crate es la **fuente de verdad** del protocolo. Los tipos TypeScript
//! de la PWA (`apps/pwa/src/protocol/generated.ts`) se generan a partir de
//! estos tipos; una prueba falla si quedan desactualizados.
//!
//! La especificación en prosa vive en `docs/contrato-eventos.md`.

pub mod client;
pub mod server;
pub mod timestamp;
pub mod validate;

pub use client::*;
pub use server::*;
pub use timestamp::Timestamp;
pub use validate::ValidationError;

/// Versión del protocolo que habla esta compilación.
pub const PROTOCOL_VERSION: u16 = 1;

/// Tamaño máximo de un frame WebSocket del cliente, en bytes.
pub const MAX_CLIENT_FRAME_BYTES: usize = 16 * 1024;

/// Longitud máxima de un mensaje de chat, en caracteres.
pub const MAX_CHAT_TEXT_CHARS: usize = 2_000;

/// Intervalo con el que el cliente debe enviar `ping`.
pub const HEARTBEAT_INTERVAL_MS: u32 = 15_000;
