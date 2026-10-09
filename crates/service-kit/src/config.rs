//! Lectura de configuración desde variables de entorno y Docker secrets.
//!
//! Convención para secretos: si existe `NOMBRE_FILE`, se lee el archivo al que
//! apunta (Docker monta los secrets en `/run/secrets/<nombre>`). Así ningún
//! secreto aparece en `docker inspect` ni en el entorno del proceso.

use std::{fmt::Display, str::FromStr};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("la variable {key} no es válida: {reason}")]
    Invalid { key: String, reason: String },
    #[error("falta la variable obligatoria {0}")]
    Missing(String),
    #[error("no se pudo leer el secreto {key} desde {path}: {source}")]
    SecretFile {
        key: String,
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// Lee y convierte una variable opcional; usa `default` si no existe.
pub fn env_or<T>(key: &str, default: T) -> Result<T, ConfigError>
where
    T: FromStr,
    T::Err: Display,
{
    match std::env::var(key) {
        Ok(raw) => parse(key, &raw),
        Err(_) => Ok(default),
    }
}

/// Lee y convierte una variable obligatoria.
pub fn env_required<T>(key: &str) -> Result<T, ConfigError>
where
    T: FromStr,
    T::Err: Display,
{
    let raw = std::env::var(key).map_err(|_| ConfigError::Missing(key.to_owned()))?;
    parse(key, &raw)
}

/// Lee un secreto: primero `KEY_FILE` (Docker secret), después `KEY`.
///
/// La variable directa solo debe usarse en desarrollo local.
pub fn secret(key: &str) -> Result<String, ConfigError> {
    let file_key = format!("{key}_FILE");
    if let Ok(path) = std::env::var(&file_key) {
        let content = std::fs::read_to_string(&path).map_err(|source| ConfigError::SecretFile {
            key: key.to_owned(),
            path,
            source,
        })?;
        return Ok(content.trim_end_matches(['\n', '\r']).to_owned());
    }
    std::env::var(key).map_err(|_| ConfigError::Missing(format!("{key} o {file_key}")))
}

fn parse<T>(key: &str, raw: &str) -> Result<T, ConfigError>
where
    T: FromStr,
    T::Err: Display,
{
    raw.trim().parse::<T>().map_err(|e| ConfigError::Invalid {
        key: key.to_owned(),
        reason: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Las pruebas usan nombres de variable únicos porque el entorno del
    // proceso es compartido entre hilos de prueba.

    #[test]
    fn env_or_uses_default_when_missing() {
        let value: u16 = env_or("SK_TEST_MISSING_PORT", 8080).unwrap();
        assert_eq!(value, 8080);
    }

    #[test]
    fn parse_reports_invalid_value() {
        let result = parse::<u16>("SK_TEST_PORT", "no-es-numero");
        assert!(matches!(result, Err(ConfigError::Invalid { .. })));
    }

    #[test]
    fn parse_trims_whitespace() {
        assert_eq!(parse::<u16>("SK_TEST_PORT", " 9000 ").unwrap(), 9000);
    }

    #[test]
    fn env_required_reports_missing() {
        let result = env_required::<String>("SK_TEST_DEFINITELY_MISSING");
        assert!(matches!(result, Err(ConfigError::Missing(_))));
    }

    #[test]
    fn secret_reports_missing() {
        assert!(matches!(
            secret("SK_TEST_NO_SECRET"),
            Err(ConfigError::Missing(_))
        ));
    }
}
