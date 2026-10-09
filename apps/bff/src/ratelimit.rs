//! Rate limiting (ADR-0020): protege recursos SIN bloquear a quien pide ayuda.
//!
//! - **Nunca se limita** `create_incident` ni los primeros mensajes de una
//!   sesión (pedir ayuda), ni los `ack`/`ping`.
//! - **Por sesión** se limita el exceso (un humano no escribe 30 mensajes por
//!   minuto). Al excederse, el comando NO se pierde: se responde
//!   `rate_limited` reintentable y la PWA lo conserva en su cola.
//! - **Por IP**, por defecto solo se OBSERVA y registra: con CGNAT miles de
//!   personas comparten una IP, y en un desastre todas piden ayuda a la vez.
//!   El límite duro por IP existe (`RATE_IP_HARD_PER_MIN`) para activarse solo
//!   ante un ataque en curso.
//! - **Falla abierta**: si Valkey no responde, se permite.

use std::time::Duration;

use protocol::ClientMessage;
use uuid::Uuid;

use crate::valkey::Valkey;

const WINDOW: Duration = Duration::from_secs(60);

/// Umbrales por minuto (configurables por entorno).
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Mensajes de chat por sesión y minuto (tras los primeros `free_messages`).
    pub chat_per_min: u64,
    /// Mensajes iniciales nunca limitados (el pedido de ayuda).
    pub free_messages: u64,
    pub location_per_min: u64,
    pub client_events_per_min: u64,
    pub call_requests_per_min: u64,
    /// Sesiones nuevas por IP y minuto a partir de las cuales se registra una alerta.
    pub ip_observe_per_min: u64,
    /// Límite duro por IP (0 = desactivado; solo ante un ataque en curso).
    pub ip_hard_per_min: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            chat_per_min: 30,
            free_messages: 5,
            location_per_min: 20,
            client_events_per_min: 60,
            call_requests_per_min: 6,
            ip_observe_per_min: 120,
            ip_hard_per_min: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// Exceso: reintentar más tarde (no se pierde).
    Limit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpDecision {
    Allow,
    /// Volumen inusual: se permite y se registra para vigilancia.
    Observe(u64),
    Reject,
}

async fn count(valkey: &Valkey, key: String) -> Option<u64> {
    let window = u64::try_from(WINDOW.as_millis()).unwrap_or(60_000);
    let n: Result<i64, _> = valkey
        .run(
            &valkey.scripts.rate,
            vec![key],
            vec![fred::types::Value::from(window.to_string())],
        )
        .await;
    n.ok().map(|n| n.unsigned_abs())
}

fn window_id() -> u64 {
    protocol::Timestamp::now().unix_millis().unsigned_abs()
        / u64::try_from(WINDOW.as_millis()).unwrap_or(60_000)
}

/// Decide para un comando de una sesión. Falla abierta.
pub async fn check_command(
    valkey: &Valkey,
    limits: &Limits,
    session_id: Uuid,
    message: &ClientMessage,
) -> Decision {
    let (kind, limit) = match message {
        // Pedir ayuda y el control del protocolo nunca se limitan.
        ClientMessage::CreateIncident(_)
        | ClientMessage::Hello(_)
        | ClientMessage::Ack(_)
        | ClientMessage::Ping(_)
        | ClientMessage::Close(_) => return Decision::Allow,
        ClientMessage::ChatMessage(_) => {
            // Los primeros mensajes de la sesión siempre pasan.
            let total = count(valkey, format!("sb:rl:{{{session_id}}}:chat_total")).await;
            if total.is_none_or(|n| n <= limits.free_messages) {
                return Decision::Allow;
            }
            ("chat", limits.chat_per_min)
        }
        ClientMessage::LocationUpdate(_) => ("loc", limits.location_per_min),
        ClientMessage::ClientEvent(_) => ("evt", limits.client_events_per_min),
        ClientMessage::VideoCallRequest(_) => ("call", limits.call_requests_per_min),
    };
    match count(
        valkey,
        format!("sb:rl:{{{session_id}}}:{kind}:{}", window_id()),
    )
    .await
    {
        Some(n) if n > limit => Decision::Limit,
        _ => Decision::Allow,
    }
}

/// Decide para una sesión nueva desde una IP. Falla abierta.
pub async fn check_new_session(valkey: &Valkey, limits: &Limits, ip: &str) -> IpDecision {
    let Some(n) = count(valkey, format!("sb:rl:ip:{ip}:{}", window_id())).await else {
        return IpDecision::Allow;
    };
    if limits.ip_hard_per_min > 0 && n > limits.ip_hard_per_min {
        IpDecision::Reject
    } else if n > limits.ip_observe_per_min {
        IpDecision::Observe(n)
    } else {
        IpDecision::Allow
    }
}
