//! Registro completo en Postgres.
//!
//! - [`hash`]: codificación canónica y hash de cada registro.
//! - [`writer`]: inserción idempotente y encadenada (la usa `record-writer`).
//! - [`checkpoint`]: checkpoints globales que encadenan todos los registros.
//! - [`verify`]: verificación completa de la integridad (la usa `sb-admin`).
//! - [`db`]: conexión con TLS verificado y migraciones.

pub mod checkpoint;
pub mod db;
pub mod hash;
#[cfg(feature = "test-support")]
pub mod test_support;
pub mod verify;
pub mod writer;

/// Hash previo del primer registro de cada cadena.
pub const GENESIS_HASH: [u8; 32] = [0; 32];
