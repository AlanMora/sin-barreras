//! Cifrado a nivel de aplicación del registro completo (ver ADR-0014).
//!
//! Esquema de sobre (*envelope encryption*):
//!
//! ```text
//!   contenido sensible ──AES-256-GCM(DEK de la sesión)──▶ texto cifrado (en Postgres)
//!   DEK de la sesión ──HPKE(llave PÚBLICA de investigación)──▶ DEK envuelta (en Postgres)
//!   DEK de la sesión ──AES-256-GCM(llave operativa)──▶ DEK operativa (en Valkey, efímera)
//! ```
//!
//! - El BFF solo tiene la llave **pública** de investigación: puede cifrar
//!   pero no descifrar el registro histórico.
//! - La llave **privada** solo la tiene la herramienta de investigación
//!   (`sb-admin`), fuera de los servicios en línea.
//! - Destruir la DEK envuelta de una sesión la vuelve ilegible para siempre
//!   (borrado criptográfico), sin tocar los registros ni romper la cadena.

use std::fmt;

use aes_gcm::{
    Aes256Gcm, KeyInit,
    aead::{Aead, Payload},
};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use hpke::{
    Deserializable, Kem as KemTrait, OpModeR, OpModeS, Serializable, aead::AesGcm256,
    kdf::HkdfSha256, kem::X25519HkdfSha256,
};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

/// Algoritmo del contenido cifrado.
pub const DATA_ALG: &str = "A256GCM";
/// Algoritmo de envoltura de la DEK hacia la llave de investigación.
pub const WRAP_ALG: &str = "HPKE-Base-X25519-SHA256-A256GCM";
/// Algoritmo de la envoltura operativa (entre réplicas del BFF).
pub const OPERATIONAL_ALG: &str = "A256GCM-OP";

const HPKE_INFO: &[u8] = b"sin-barreras/dek-wrap/v1";
const NONCE_LEN: usize = 12;

type Kem = X25519HkdfSha256;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CryptoError {
    #[error("no hay aleatoriedad del sistema")]
    Random,
    #[error("no se pudo cifrar")]
    Seal,
    #[error("no se pudo descifrar: llave incorrecta o datos alterados")]
    Open,
    #[error("llave mal formada: {0}")]
    BadKey(&'static str),
    #[error("algoritmo no soportado: {0}")]
    UnsupportedAlg(String),
    #[error("no hay llave con id {0}")]
    UnknownKey(String),
}

fn random<const N: usize>() -> Result<[u8; N], CryptoError> {
    let mut bytes = [0_u8; N];
    getrandom::fill(&mut bytes).map_err(|_| CryptoError::Random)?;
    Ok(bytes)
}

// ---------------------------------------------------------------------------
// Llave de datos (DEK) y contenido
// ---------------------------------------------------------------------------

/// Llave de datos de 256 bits. Se borra de memoria al soltarse.
#[derive(Clone, Zeroize, ZeroizeOnDrop, PartialEq, Eq)]
pub struct DataKey([u8; 32]);

impl fmt::Debug for DataKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DataKey(<oculta>)")
    }
}

impl DataKey {
    pub fn generate() -> Result<Self, CryptoError> {
        Ok(Self(random()?))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        let array: [u8; 32] = bytes
            .try_into()
            .map_err(|_| CryptoError::BadKey("se esperaban 32 bytes"))?;
        Ok(Self(array))
    }

    fn cipher(&self) -> Result<Aes256Gcm, CryptoError> {
        Aes256Gcm::new_from_slice(&self.0).map_err(|_| CryptoError::BadKey("AES-256"))
    }
}

/// Contenido cifrado: nonce aleatorio de 96 bits + texto cifrado con etiqueta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sealed {
    pub nonce: [u8; NONCE_LEN],
    pub ciphertext: Vec<u8>,
}

/// Cifra con AES-256-GCM. `aad` liga el texto cifrado a su contexto (id de
/// evento, sesión y tipo): moverlo a otro registro hace fallar el descifrado.
pub fn seal(key: &DataKey, aad: &[u8], plaintext: &[u8]) -> Result<Sealed, CryptoError> {
    let nonce: [u8; NONCE_LEN] = random()?;
    let ciphertext = key
        .cipher()?
        .encrypt(
            &nonce.into(),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| CryptoError::Seal)?;
    Ok(Sealed { nonce, ciphertext })
}

pub fn open(key: &DataKey, aad: &[u8], sealed: &Sealed) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    key.cipher()?
        .decrypt(
            &sealed.nonce.into(),
            Payload {
                msg: &sealed.ciphertext,
                aad,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| CryptoError::Open)
}

// ---------------------------------------------------------------------------
// Llave de investigación (HPKE)
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct RecipientPublicKey {
    pub key_id: String,
    key: <Kem as KemTrait>::PublicKey,
}

pub struct RecipientPrivateKey {
    pub key_id: String,
    key: <Kem as KemTrait>::PrivateKey,
}

impl fmt::Debug for RecipientPublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RecipientPublicKey({})", self.key_id)
    }
}

impl fmt::Debug for RecipientPrivateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RecipientPrivateKey({}, <oculta>)", self.key_id)
    }
}

/// Genera un par de llaves de investigación. La privada debe guardarse fuera
/// de los servidores en línea.
pub fn generate_recipient_keypair(key_id: &str) -> (RecipientPrivateKey, RecipientPublicKey) {
    let (private, public) = Kem::gen_keypair();
    (
        RecipientPrivateKey {
            key_id: key_id.to_owned(),
            key: private,
        },
        RecipientPublicKey {
            key_id: key_id.to_owned(),
            key: public,
        },
    )
}

impl RecipientPublicKey {
    /// Formato de configuración: `<id>:<base64>`.
    pub fn parse(spec: &str) -> Result<Self, CryptoError> {
        let (key_id, encoded) = spec
            .trim()
            .split_once(':')
            .ok_or(CryptoError::BadKey("formato id:base64"))?;
        let bytes = B64
            .decode(encoded)
            .map_err(|_| CryptoError::BadKey("base64"))?;
        let key = <Kem as KemTrait>::PublicKey::from_bytes(&bytes)
            .map_err(|_| CryptoError::BadKey("X25519"))?;
        Ok(Self {
            key_id: key_id.to_owned(),
            key,
        })
    }

    pub fn to_spec(&self) -> String {
        format!("{}:{}", self.key_id, B64.encode(self.key.to_bytes()))
    }
}

impl RecipientPrivateKey {
    pub fn parse(spec: &str) -> Result<Self, CryptoError> {
        let (key_id, encoded) = spec
            .trim()
            .split_once(':')
            .ok_or(CryptoError::BadKey("formato id:base64"))?;
        let bytes = Zeroizing::new(
            B64.decode(encoded)
                .map_err(|_| CryptoError::BadKey("base64"))?,
        );
        let key = <Kem as KemTrait>::PrivateKey::from_bytes(&bytes)
            .map_err(|_| CryptoError::BadKey("X25519"))?;
        Ok(Self {
            key_id: key_id.to_owned(),
            key,
        })
    }

    pub fn to_spec(&self) -> Zeroizing<String> {
        Zeroizing::new(format!(
            "{}:{}",
            self.key_id,
            B64.encode(self.key.to_bytes())
        ))
    }

    pub fn public(&self) -> RecipientPublicKey {
        RecipientPublicKey {
            key_id: self.key_id.clone(),
            key: Kem::sk_to_pk(&self.key),
        }
    }
}

/// DEK envuelta para la llave de investigación.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrappedKey {
    pub alg: String,
    pub recipient_key_id: String,
    /// Llave efímera encapsulada (HPKE `enc`).
    pub encapped: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

/// Envuelve una DEK para la llave pública de investigación. `context`
/// (p. ej. el id de sesión) queda ligado como dato asociado.
pub fn wrap_for_recipient(
    recipient: &RecipientPublicKey,
    key: &DataKey,
    context: &[u8],
) -> Result<WrappedKey, CryptoError> {
    let (encapped, ciphertext) = hpke::single_shot_seal::<AesGcm256, HkdfSha256, Kem>(
        &OpModeS::Base,
        &recipient.key,
        HPKE_INFO,
        &key.0,
        context,
    )
    .map_err(|_| CryptoError::Seal)?;
    Ok(WrappedKey {
        alg: WRAP_ALG.to_owned(),
        recipient_key_id: recipient.key_id.clone(),
        encapped: encapped.to_bytes().to_vec(),
        ciphertext,
    })
}

pub fn unwrap_with_recipient(
    private: &RecipientPrivateKey,
    wrapped: &WrappedKey,
    context: &[u8],
) -> Result<DataKey, CryptoError> {
    if wrapped.alg != WRAP_ALG {
        return Err(CryptoError::UnsupportedAlg(wrapped.alg.clone()));
    }
    if wrapped.recipient_key_id != private.key_id {
        return Err(CryptoError::UnknownKey(wrapped.recipient_key_id.clone()));
    }
    let encapped = <Kem as KemTrait>::EncappedKey::from_bytes(&wrapped.encapped)
        .map_err(|_| CryptoError::Open)?;
    let plain = Zeroizing::new(
        hpke::single_shot_open::<AesGcm256, HkdfSha256, Kem>(
            &OpModeR::Base,
            &private.key,
            &encapped,
            HPKE_INFO,
            &wrapped.ciphertext,
            context,
        )
        .map_err(|_| CryptoError::Open)?,
    );
    DataKey::from_bytes(&plain)
}

// ---------------------------------------------------------------------------
// Llave operativa (compartir la DEK entre réplicas del BFF vía Valkey)
// ---------------------------------------------------------------------------

/// Llave simétrica con la que las réplicas del BFF comparten la DEK de una
/// sesión activa a través de Valkey. Un volcado de Valkey sin esta llave no
/// revela las DEK.
#[derive(Clone)]
pub struct OperationalKey {
    pub key_id: String,
    key: DataKey,
}

impl fmt::Debug for OperationalKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OperationalKey({}, <oculta>)", self.key_id)
    }
}

impl OperationalKey {
    pub fn new(key_id: &str, bytes: &[u8]) -> Result<Self, CryptoError> {
        Ok(Self {
            key_id: key_id.to_owned(),
            key: DataKey::from_bytes(bytes)?,
        })
    }

    /// `<id>:<base64 de 32 bytes>`
    pub fn parse(spec: &str) -> Result<Self, CryptoError> {
        let (key_id, encoded) = spec
            .trim()
            .split_once(':')
            .ok_or(CryptoError::BadKey("formato id:base64"))?;
        let bytes = Zeroizing::new(
            B64.decode(encoded)
                .map_err(|_| CryptoError::BadKey("base64"))?,
        );
        Self::new(key_id, &bytes)
    }

    /// Envuelve una DEK: `<id>.<nonce b64>.<ct b64>`.
    pub fn wrap(&self, key: &DataKey, context: &[u8]) -> Result<String, CryptoError> {
        let sealed = seal(&self.key, context, &key.0)?;
        Ok(format!(
            "{}.{}.{}",
            self.key_id,
            B64.encode(sealed.nonce),
            B64.encode(&sealed.ciphertext)
        ))
    }
}

/// Desenvuelve con la llave operativa que corresponda al id (permite rotar).
pub fn unwrap_operational(
    keys: &[OperationalKey],
    wrapped: &str,
    context: &[u8],
) -> Result<DataKey, CryptoError> {
    let mut parts = wrapped.splitn(3, '.');
    let (Some(key_id), Some(nonce), Some(ciphertext)) = (parts.next(), parts.next(), parts.next())
    else {
        return Err(CryptoError::BadKey("formato id.nonce.ct"));
    };
    let op = keys
        .iter()
        .find(|k| k.key_id == key_id)
        .ok_or_else(|| CryptoError::UnknownKey(key_id.to_owned()))?;
    let nonce: [u8; NONCE_LEN] = B64
        .decode(nonce)
        .ok()
        .and_then(|n| n.try_into().ok())
        .ok_or(CryptoError::BadKey("nonce"))?;
    let ciphertext = B64
        .decode(ciphertext)
        .map_err(|_| CryptoError::BadKey("base64"))?;
    let plain = open(&op.key, context, &Sealed { nonce, ciphertext })?;
    DataKey::from_bytes(&plain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_and_open_roundtrip() {
        let key = DataKey::generate().unwrap();
        let sealed = seal(
            &key,
            b"evt-1|ses-1|citizen_message",
            "Necesito ayuda 🙏".as_bytes(),
        )
        .unwrap();
        assert_ne!(sealed.ciphertext, "Necesito ayuda 🙏".as_bytes());
        let plain = open(&key, b"evt-1|ses-1|citizen_message", &sealed).unwrap();
        assert_eq!(plain.as_slice(), "Necesito ayuda 🙏".as_bytes());
    }

    #[test]
    fn nonces_are_unique() {
        let key = DataKey::generate().unwrap();
        let a = seal(&key, b"", b"x").unwrap();
        let b = seal(&key, b"", b"x").unwrap();
        assert_ne!(a.nonce, b.nonce);
        assert_ne!(a.ciphertext, b.ciphertext);
    }

    #[test]
    fn tampering_or_wrong_context_fails() {
        let key = DataKey::generate().unwrap();
        let mut sealed = seal(&key, b"evt-1", b"ubicacion").unwrap();
        // Otro contexto (mover el texto cifrado a otro registro).
        assert_eq!(open(&key, b"evt-2", &sealed), Err(CryptoError::Open));
        // Otra llave.
        assert_eq!(
            open(&DataKey::generate().unwrap(), b"evt-1", &sealed),
            Err(CryptoError::Open)
        );
        // Un bit alterado.
        if let Some(byte) = sealed.ciphertext.first_mut() {
            *byte ^= 1;
        }
        assert_eq!(open(&key, b"evt-1", &sealed), Err(CryptoError::Open));
    }

    #[test]
    fn hpke_wrap_roundtrip_and_context_binding() {
        let (private, public) = generate_recipient_keypair("inv-2026");
        let dek = DataKey::generate().unwrap();
        let wrapped = wrap_for_recipient(&public, &dek, b"session-A").unwrap();
        assert_eq!(wrapped.alg, WRAP_ALG);
        assert_eq!(wrapped.recipient_key_id, "inv-2026");
        assert_eq!(
            unwrap_with_recipient(&private, &wrapped, b"session-A").unwrap(),
            dek
        );
        assert_eq!(
            unwrap_with_recipient(&private, &wrapped, b"session-B"),
            Err(CryptoError::Open)
        );
        let (other, _) = generate_recipient_keypair("inv-2026");
        assert_eq!(
            unwrap_with_recipient(&other, &wrapped, b"session-A"),
            Err(CryptoError::Open)
        );
    }

    #[test]
    fn key_specs_roundtrip() {
        let (private, public) = generate_recipient_keypair("k1");
        let public2 = RecipientPublicKey::parse(&public.to_spec()).unwrap();
        let private2 = RecipientPrivateKey::parse(&private.to_spec()).unwrap();
        let dek = DataKey::generate().unwrap();
        let wrapped = wrap_for_recipient(&public2, &dek, b"ctx").unwrap();
        assert_eq!(
            unwrap_with_recipient(&private2, &wrapped, b"ctx").unwrap(),
            dek
        );
        assert_eq!(private2.public().to_spec(), public.to_spec());
        assert!(RecipientPublicKey::parse("sin-separador").is_err());
    }

    #[test]
    fn operational_wrap_supports_rotation() {
        let old = OperationalKey::new("op1", &[1; 32]).unwrap();
        let new = OperationalKey::new("op2", &[2; 32]).unwrap();
        let dek = DataKey::generate().unwrap();
        let wrapped_old = old.wrap(&dek, b"s").unwrap();
        // Tras rotar, la llave vieja sigue disponible para desenvolver.
        let keys = vec![new.clone(), old];
        assert_eq!(unwrap_operational(&keys, &wrapped_old, b"s").unwrap(), dek);
        assert!(unwrap_operational(&[new], &wrapped_old, b"s").is_err());
    }

    #[test]
    fn debug_never_prints_key_material() {
        let dek = DataKey::generate().unwrap();
        assert_eq!(format!("{dek:?}"), "DataKey(<oculta>)");
        let (private, _) = generate_recipient_keypair("k");
        assert!(!format!("{private:?}").contains(&private.to_spec()[3..]));
    }
}
