//! Firma y verificación de los códigos QR de los puntos de entrada.
//!
//! URL del QR: `https://<dominio>/?t=TOT-0142&k=1&s=<firma>`
//! - `t`: identificador del punto.
//! - `k`: id de la llave con que se firmó (permite rotar llaves sin reimprimir
//!   todos los QR a la vez).
//! - `s`: HMAC-SHA256(llave, "sb-qr-v1|" + t), truncado a 128 bits, base64url.
//!
//! Un QR inválido NO bloquea la ayuda (ADR-0007): la sesión sigue como
//! "no verificada".

use std::collections::{HashMap, HashSet};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use protocol::{EntryVerification, QrEntry};
use sha2::Sha256;

const DOMAIN: &str = "sb-qr-v1|";
const SIGNATURE_BYTES: usize = 16;
pub const DEFAULT_KEY_ID: &str = "1";

#[derive(Clone, Default)]
pub struct QrVerifier {
    keys: HashMap<String, Vec<u8>>,
    /// Puntos revocados (QR robado, dañado o retirado).
    revoked: HashSet<String>,
}

impl QrVerifier {
    pub fn new(keys: HashMap<String, Vec<u8>>) -> Self {
        Self {
            keys,
            revoked: HashSet::new(),
        }
    }

    pub fn with_revoked(mut self, revoked: HashSet<String>) -> Self {
        self.revoked = revoked;
        self
    }

    pub fn verify(&self, entry: Option<&QrEntry>) -> EntryVerification {
        let Some(entry) = entry else {
            return EntryVerification::None;
        };
        if self.revoked.contains(&entry.point_id) {
            return EntryVerification::Unverified;
        }
        let key_id = entry.key_id.as_deref().unwrap_or(DEFAULT_KEY_ID);
        let Some(key) = self.keys.get(key_id) else {
            return EntryVerification::Unverified;
        };
        let Ok(signature) = URL_SAFE_NO_PAD.decode(&entry.signature) else {
            return EntryVerification::Unverified;
        };
        if signature.len() != SIGNATURE_BYTES {
            return EntryVerification::Unverified;
        }
        let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(key) else {
            return EntryVerification::Unverified;
        };
        mac.update(DOMAIN.as_bytes());
        mac.update(entry.point_id.as_bytes());
        // Comparación en tiempo constante de los primeros 16 bytes.
        match mac.verify_truncated_left(&signature) {
            Ok(()) => EntryVerification::Verified,
            Err(_) => EntryVerification::Unverified,
        }
    }

    /// Firma un punto (para generar los QR que se imprimen).
    pub fn sign(&self, key_id: &str, point_id: &str) -> Option<String> {
        let key = self.keys.get(key_id)?;
        let mut mac = Hmac::<Sha256>::new_from_slice(key).ok()?;
        mac.update(DOMAIN.as_bytes());
        mac.update(point_id.as_bytes());
        let tag = mac.finalize().into_bytes();
        Some(URL_SAFE_NO_PAD.encode(&tag[..SIGNATURE_BYTES]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verifier() -> QrVerifier {
        QrVerifier::new(HashMap::from([
            ("1".to_owned(), vec![7_u8; 32]),
            ("2".to_owned(), vec![9_u8; 32]),
        ]))
    }

    fn entry(point: &str, key: Option<&str>, sig: &str) -> QrEntry {
        QrEntry {
            point_id: point.into(),
            key_id: key.map(Into::into),
            signature: sig.into(),
        }
    }

    #[test]
    fn verifies_valid_signature_with_each_key() {
        let v = verifier();
        for key in ["1", "2"] {
            let sig = v.sign(key, "TOT-0142").unwrap();
            assert_eq!(sig.len(), 22);
            assert_eq!(
                v.verify(Some(&entry("TOT-0142", Some(key), &sig))),
                EntryVerification::Verified
            );
        }
    }

    #[test]
    fn default_key_id_is_1() {
        let v = verifier();
        let sig = v.sign("1", "TOT-0142").unwrap();
        assert_eq!(
            v.verify(Some(&entry("TOT-0142", None, &sig))),
            EntryVerification::Verified
        );
    }

    #[test]
    fn rejects_tampering() {
        let v = verifier();
        let sig = v.sign("1", "TOT-0142").unwrap();
        // Otro punto con la misma firma.
        assert_eq!(
            v.verify(Some(&entry("TOT-0143", Some("1"), &sig))),
            EntryVerification::Unverified
        );
        // Firma con la llave equivocada.
        assert_eq!(
            v.verify(Some(&entry("TOT-0142", Some("2"), &sig))),
            EntryVerification::Unverified
        );
        // Llave desconocida, basura y longitud incorrecta.
        assert_eq!(
            v.verify(Some(&entry("TOT-0142", Some("9"), &sig))),
            EntryVerification::Unverified
        );
        assert_eq!(
            v.verify(Some(&entry("TOT-0142", Some("1"), "%%%"))),
            EntryVerification::Unverified
        );
        assert_eq!(
            v.verify(Some(&entry("TOT-0142", Some("1"), "AAAA"))),
            EntryVerification::Unverified
        );
    }

    #[test]
    fn revoked_points_are_unverified_even_with_valid_signature() {
        let v = verifier().with_revoked(HashSet::from(["TOT-0142".to_owned()]));
        let sig = v.sign("1", "TOT-0142").unwrap();
        assert_eq!(
            v.verify(Some(&entry("TOT-0142", Some("1"), &sig))),
            EntryVerification::Unverified
        );
    }

    #[test]
    fn no_entry_is_none() {
        assert_eq!(verifier().verify(None), EntryVerification::None);
    }
}
