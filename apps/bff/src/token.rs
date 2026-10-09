//! Token anónimo de sesión.
//!
//! - 256 bits aleatorios del sistema operativo, codificados en base64url.
//! - En Valkey solo se guarda su SHA-256: quien lea Valkey no puede
//!   suplantar una sesión.
//! - Se rota en cada `welcome`; el anterior deja de servir (ver ADR-0010).

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
#[error("no se pudo obtener aleatoriedad del sistema: {0}")]
pub struct TokenError(String);

pub fn generate() -> Result<String, TokenError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| TokenError(e.to_string()))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

pub fn hash(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_unique_and_urlsafe() {
        let a = generate().unwrap();
        let b = generate().unwrap();
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
        assert!(
            a.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
    }

    #[test]
    fn hash_is_stable_hex() {
        assert_eq!(hash("abc").len(), 64);
        assert_eq!(hash("abc"), hash("abc"));
        assert_ne!(hash("abc"), hash("abd"));
    }
}
