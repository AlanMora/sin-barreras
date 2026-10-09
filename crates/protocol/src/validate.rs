//! Validación estricta de los mensajes del cliente.
//!
//! Dos pasos, siguiendo el ADR-0007 ("ninguna validación bloquea la ayuda"):
//! 1. [`ClientMessage::sanitize`] descarta campos **opcionales** inválidos
//!    (trazas, ajustes, QR mal formado) y devuelve cuáles descartó, para
//!    registrarlo. El mensaje sigue su curso.
//! 2. [`ClientMessage::validate`] rechaza solo lo que hace imposible
//!    interpretar el mensaje (texto vacío, coordenadas imposibles…).

use crate::{ChatKind, ClientMessage, Hello, MAX_CHAT_TEXT_CHARS, QrEntry};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("campo `{field}` inválido: {reason}")]
pub struct ValidationError {
    pub field: &'static str,
    pub reason: &'static str,
}

fn invalid(field: &'static str, reason: &'static str) -> ValidationError {
    ValidationError { field, reason }
}

impl ClientMessage {
    /// Descarta campos opcionales inválidos. Devuelve los nombres de los
    /// campos descartados (vacío si todo estaba bien).
    pub fn sanitize(&mut self) -> Vec<&'static str> {
        let mut dropped = Vec::new();
        let traceparent = match self {
            Self::Hello(m) => &mut m.traceparent,
            Self::CreateIncident(m) => &mut m.traceparent,
            Self::ChatMessage(m) => &mut m.traceparent,
            Self::LocationUpdate(m) => &mut m.traceparent,
            Self::VideoCallRequest(m) => &mut m.traceparent,
            Self::ClientEvent(m) => &mut m.traceparent,
            Self::Ack(_) | Self::Ping(_) | Self::Close(_) => &mut None,
        };
        if traceparent.as_deref().is_some_and(|t| !is_traceparent(t)) {
            *traceparent = None;
            dropped.push("traceparent");
        }

        match self {
            Self::Hello(hello) => sanitize_hello(hello, &mut dropped),
            Self::CreateIncident(m)
                if m.language.as_deref().is_some_and(|l| !is_language_tag(l)) =>
            {
                m.language = None;
                dropped.push("language");
            }
            Self::LocationUpdate(m)
                if m.accuracy_m
                    .is_some_and(|a| !(a.is_finite() && (0.0..=100_000.0).contains(&a))) =>
            {
                m.accuracy_m = None;
                dropped.push("accuracy_m");
            }
            Self::ClientEvent(m)
                if m.detail
                    .as_deref()
                    .is_some_and(|d| !is_short_plain_text(d, 200)) =>
            {
                m.detail = None;
                dropped.push("detail");
            }
            _ => {}
        }
        dropped
    }

    /// Rechaza mensajes imposibles de interpretar.
    pub fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::Hello(m) => {
                if m.protocol_version == 0 {
                    return Err(invalid("protocol_version", "debe ser mayor que cero"));
                }
                if let Some(resume) = &m.resume
                    && !is_token(&resume.session_token)
                {
                    return Err(invalid("resume.session_token", "formato inválido"));
                }
                Ok(())
            }
            Self::CreateIncident(_)
            | Self::VideoCallRequest(_)
            | Self::Ping(_)
            | Self::Close(_) => Ok(()),
            Self::ChatMessage(m) => {
                let trimmed = m.text.trim();
                if trimmed.is_empty() {
                    return Err(invalid("text", "no puede estar vacío"));
                }
                if m.text.chars().count() > MAX_CHAT_TEXT_CHARS {
                    return Err(invalid("text", "excede la longitud máxima"));
                }
                if m.text
                    .chars()
                    .any(|c| c.is_control() && c != '\n' && c != '\t')
                {
                    return Err(invalid("text", "contiene caracteres de control"));
                }
                match (m.kind, m.quick_reply) {
                    (ChatKind::QuickReply, None) => Err(invalid(
                        "quick_reply",
                        "obligatorio cuando kind = quick_reply",
                    )),
                    (ChatKind::Text, Some(_)) => Err(invalid(
                        "quick_reply",
                        "solo se permite cuando kind = quick_reply",
                    )),
                    _ => Ok(()),
                }
            }
            Self::LocationUpdate(m) => {
                if !(m.latitude.is_finite() && (-90.0..=90.0).contains(&m.latitude)) {
                    return Err(invalid("latitude", "fuera de rango"));
                }
                if !(m.longitude.is_finite() && (-180.0..=180.0).contains(&m.longitude)) {
                    return Err(invalid("longitude", "fuera de rango"));
                }
                Ok(())
            }
            Self::ClientEvent(_) => Ok(()),
            Self::Ack(m) => {
                if m.seq == 0 {
                    return Err(invalid("seq", "debe ser mayor que cero"));
                }
                Ok(())
            }
        }
    }
}

fn sanitize_hello(hello: &mut Hello, dropped: &mut Vec<&'static str>) {
    if !is_short_token(&hello.app_version, 32, |c| {
        c.is_ascii_alphanumeric() || ".+-".contains(c)
    }) {
        hello.app_version = "desconocida".to_owned();
        dropped.push("app_version");
    }
    if !is_language_tag(&hello.language) {
        hello.language = "es-MX".to_owned();
        dropped.push("language");
    }
    if hello.entry.as_ref().is_some_and(|e| !is_valid_qr_entry(e)) {
        hello.entry = None;
        dropped.push("entry");
    }
    if hello
        .accessibility
        .as_ref()
        .is_some_and(|a| !(a.font_scale.is_finite() && (0.5..=3.0).contains(&a.font_scale)))
    {
        hello.accessibility = None;
        dropped.push("accessibility");
    }
}

/// `TOT-0142`, `ESC-17`, `HOS-A3`…: prefijo de 2 a 5 letras y código de 1 a 12.
pub fn is_point_id(value: &str) -> bool {
    let Some((prefix, code)) = value.split_once('-') else {
        return false;
    };
    (2..=5).contains(&prefix.len())
        && prefix.chars().all(|c| c.is_ascii_uppercase())
        && (1..=12).contains(&code.len())
        && code
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

fn is_valid_qr_entry(entry: &QrEntry) -> bool {
    is_point_id(&entry.point_id)
        && entry
            .key_id
            .as_deref()
            .is_none_or(|k| is_short_token(k, 16, |c| c.is_ascii_alphanumeric()))
        && is_short_token(&entry.signature, 128, is_base64url_char)
}

fn is_base64url_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// Token de sesión: `<base64url>`, hasta 128 caracteres.
fn is_token(value: &str) -> bool {
    is_short_token(value, 128, is_base64url_char)
}

fn is_short_token(value: &str, max: usize, allowed: impl Fn(char) -> bool) -> bool {
    !value.is_empty() && value.len() <= max && value.chars().all(allowed)
}

fn is_short_plain_text(value: &str, max_chars: usize) -> bool {
    value.chars().count() <= max_chars && !value.chars().any(char::is_control)
}

/// Validación simple de etiqueta BCP 47 (`es`, `es-MX`, `zh-Hant-TW`).
pub fn is_language_tag(value: &str) -> bool {
    if value.is_empty() || value.len() > 35 {
        return false;
    }
    let mut parts = value.split('-');
    let primary_ok = parts
        .next()
        .is_some_and(|p| (2..=3).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphabetic()));
    primary_ok
        && parts.all(|p| (2..=8).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// Contexto W3C: `00-<trace-id 32 hex>-<parent-id 16 hex>-<flags 2 hex>`.
pub fn is_traceparent(value: &str) -> bool {
    let parts: Vec<&str> = value.split('-').collect();
    let hex = |s: &str| {
        s.chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    };
    matches!(parts.as_slice(), [version, trace, parent, flags]
        if *version == "00"
            && trace.len() == 32 && hex(trace) && trace.chars().any(|c| c != '0')
            && parent.len() == 16 && hex(parent) && parent.chars().any(|c| c != '0')
            && flags.len() == 2 && hex(flags))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_ids() {
        assert!(is_point_id("TOT-0142"));
        assert!(is_point_id("HOS-A3"));
        assert!(!is_point_id("tot-0142"));
        assert!(!is_point_id("TOT0142"));
        assert!(!is_point_id("TOT-<script>"));
    }

    #[test]
    fn language_tags() {
        assert!(is_language_tag("es"));
        assert!(is_language_tag("es-MX"));
        assert!(is_language_tag("zh-Hant-TW"));
        assert!(!is_language_tag("e"));
        assert!(!is_language_tag("es_MX"));
        assert!(!is_language_tag(""));
    }

    #[test]
    fn traceparents() {
        assert!(is_traceparent(
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
        ));
        assert!(!is_traceparent(
            "00-00000000000000000000000000000000-00f067aa0ba902b7-01"
        ));
        assert!(!is_traceparent("garbage"));
    }
}
