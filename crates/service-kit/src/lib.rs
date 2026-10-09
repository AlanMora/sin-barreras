//! Piezas comunes a todos los servicios Rust de Sin Barreras.
//!
//! Se mantienen aquí para que BFF, record-writer y c5-mock se comporten igual
//! en logs, configuración, health checks y apagado.

pub mod config;
pub mod health;
pub mod shutdown;
pub mod telemetry;
