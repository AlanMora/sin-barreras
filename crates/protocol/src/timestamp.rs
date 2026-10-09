//! Marca de tiempo del protocolo: RFC 3339 en UTC con milisegundos.
//!
//! Siempre se serializa como `2026-10-09T18:37:19.842Z`. Al leer se acepta
//! cualquier desplazamiento RFC 3339 y se normaliza a UTC.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::{
    OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339, macros::format_description,
};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, TS)]
#[ts(type = "string")]
pub struct Timestamp(OffsetDateTime);

impl Timestamp {
    /// Hora actual del servidor, truncada a milisegundos.
    pub fn now() -> Self {
        Self::from(OffsetDateTime::now_utc())
    }

    pub fn as_offset_date_time(&self) -> OffsetDateTime {
        self.0
    }

    /// Milisegundos desde la época Unix.
    pub fn unix_millis(&self) -> i64 {
        i64::try_from(self.0.unix_timestamp_nanos() / 1_000_000).unwrap_or(i64::MAX)
    }

    pub fn from_unix_millis(ms: i64) -> Option<Self> {
        OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000)
            .ok()
            .map(Self)
    }
}

impl From<OffsetDateTime> for Timestamp {
    fn from(value: OffsetDateTime) -> Self {
        let utc = value.to_offset(UtcOffset::UTC);
        let millis = utc.millisecond();
        // Truncar a milisegundos: es la precisión que exige el registro.
        Self(utc.replace_millisecond(millis).unwrap_or(utc))
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let format = format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
        );
        match self.0.format(&format) {
            Ok(s) => f.write_str(&s),
            Err(_) => Err(fmt::Error),
        }
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        if raw.len() > 40 {
            return Err(serde::de::Error::custom("marca de tiempo demasiado larga"));
        }
        OffsetDateTime::parse(&raw, &Rfc3339)
            .map(Self::from)
            .map_err(|e| {
                serde::de::Error::custom(format!("marca de tiempo RFC 3339 inválida: {e}"))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_utc_with_milliseconds() {
        let ts: Timestamp = serde_json::from_str("\"2026-10-09T12:37:19.842123-06:00\"").unwrap();
        assert_eq!(
            serde_json::to_string(&ts).unwrap(),
            "\"2026-10-09T18:37:19.842Z\""
        );
    }

    #[test]
    fn rejects_non_rfc3339() {
        assert!(serde_json::from_str::<Timestamp>("\"09/10/2026\"").is_err());
    }

    #[test]
    fn unix_millis_roundtrip() {
        let ts = Timestamp::now();
        assert_eq!(Timestamp::from_unix_millis(ts.unix_millis()), Some(ts));
    }
}
