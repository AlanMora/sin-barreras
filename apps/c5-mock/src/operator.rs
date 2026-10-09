//! Operador simulado: responde como lo haría una persona del C5.

use std::{sync::Arc, time::Duration};

use protocol::IncidentState;
use serde_json::json;
use uuid::Uuid;

use crate::state::{MockState, random_between};

async fn pause(min_ms: u64, max_ms: u64) {
    tokio::time::sleep(Duration::from_millis(random_between(min_ms, max_ms))).await;
}

fn english(language: &str) -> bool {
    language.starts_with("en")
}

pub fn set_status(
    state: &MockState,
    incident_id: &str,
    status: IncidentState,
    eta_seconds: Option<u32>,
) {
    state.with_incident(incident_id, |i| i.status = status);
    state.publish(
        incident_id,
        json!({ "type": "incident_status", "status": status, "eta_seconds": eta_seconds }),
    );
}

pub fn say(state: &MockState, incident_id: &str, text: &str) {
    let message_id = format!("OP-{}", Uuid::new_v4().simple());
    state.publish(
        incident_id,
        json!({ "type": "operator_message", "message_id": message_id, "text": text }),
    );
}

/// Al crear un incidente: el operador toma el caso y saluda.
pub fn on_incident_created(
    state: Arc<MockState>,
    incident_id: String,
    language: String,
    channel: String,
) {
    if !state.config.auto_operator {
        return;
    }
    tokio::spawn(async move {
        pause(800, 2_000).await;
        set_status(&state, &incident_id, IncidentState::Attending, None);
        pause(500, 1_500).await;
        let greeting = match (english(&language), channel.as_str()) {
            (true, "silent") => {
                "C5 Zapopan here. Reply with the buttons if you can't talk. Are you safe right now?"
            }
            (true, _) => "C5 Zapopan here. We are with you. What is happening and where are you?",
            (false, "silent") => {
                "Te atiende el C5 Zapopan. Si no puedes hablar, responde con los botones. ¿Estás a salvo ahora?"
            }
            (false, _) => {
                "Te atiende el C5 Zapopan. Estamos contigo. ¿Qué está pasando y dónde estás?"
            }
        };
        say(&state, &incident_id, greeting);
    });
}

/// Al recibir un mensaje del ciudadano: lo lee, responde y, al segundo
/// mensaje, despacha una unidad.
pub fn on_citizen_message(
    state: Arc<MockState>,
    incident_id: String,
    idempotency_key: Uuid,
    quick_reply: Option<String>,
    count: u32,
    language: String,
) {
    if !state.config.auto_operator {
        return;
    }
    tokio::spawn(async move {
        pause(600, 1_500).await;
        state.publish(
            &incident_id,
            json!({ "type": "message_status", "idempotency_key": idempotency_key, "status": "read" }),
        );
        pause(1_500, 3_500).await;
        let en = english(&language);
        let reply = match (quick_reply.as_deref(), en) {
            (Some("in_danger"), false) => {
                "Entendido, estás en peligro. Mantén el teléfono en silencio. Ya enviamos ayuda."
            }
            (Some("in_danger"), true) => {
                "Understood, you are in danger. Keep your phone silent. Help is on the way."
            }
            (Some("cannot_speak"), false) => "Está bien, no necesitas hablar. Seguimos por aquí.",
            (Some("cannot_speak"), true) => {
                "That's okay, you don't need to talk. We'll keep going here."
            }
            (Some("being_followed"), false) => {
                "Si puedes, entra a un lugar con gente. Comparte tu ubicación; vamos para allá."
            }
            (Some("being_followed"), true) => {
                "If you can, go into a place with people. Share your location; we're on our way."
            }
            (Some("injured"), false) => {
                "Enviamos una ambulancia. ¿Dónde te duele? ¿Estás sangrando?"
            }
            (Some("injured"), true) => {
                "We are sending an ambulance. Where does it hurt? Are you bleeding?"
            }
            (_, false) => "Gracias, recibido. ¿Puedes darnos una referencia de tu ubicación?",
            (_, true) => "Thank you, received. Can you give us a landmark near you?",
        };
        say(&state, &incident_id, reply);
        if count == 2
            || quick_reply.as_deref() == Some("in_danger")
            || quick_reply.as_deref() == Some("injured")
        {
            pause(1_000, 2_500).await;
            set_status(
                &state,
                &incident_id,
                IncidentState::UnitEnRoute,
                Some(u32::try_from(random_between(240, 720)).unwrap_or(480)),
            );
        }
    });
}

/// Videollamada: el sistema de video del otro equipo prepara la sala.
pub fn on_video_call(
    state: Arc<MockState>,
    incident_id: String,
    idempotency_key: Uuid,
    video_call_id: String,
) {
    tokio::spawn(async move {
        pause(1_000, 2_500).await;
        state.publish(
            &incident_id,
            json!({
                "type": "video_call",
                "idempotency_key": idempotency_key,
                "status": "ready",
                "join": {
                    "provider": "mock-video",
                    "video_call_id": video_call_id,
                    "room_url": format!("https://video.mock.invalid/rooms/{video_call_id}"),
                    "token": Uuid::new_v4().simple().to_string(),
                    "interpreter": "LSM",
                },
            }),
        );
    });
}
