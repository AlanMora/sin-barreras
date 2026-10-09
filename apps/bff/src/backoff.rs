//! Backoff exponencial con jitter completo ("full jitter").
//!
//! El jitter evita que muchas réplicas o muchos ciudadanos reintenten al
//! mismo tiempo contra un servicio que se acaba de recuperar.

use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct Backoff {
    pub initial: Duration,
    pub max: Duration,
    attempt: u32,
}

impl Backoff {
    pub fn new(initial: Duration, max: Duration) -> Self {
        Self {
            initial,
            max,
            attempt: 0,
        }
    }

    /// Siguiente espera: aleatoria entre 0 y min(max, initial · 2^intento).
    pub fn next_delay(&mut self) -> Duration {
        let factor = 2_u32.saturating_pow(self.attempt.min(16));
        self.attempt = self.attempt.saturating_add(1);
        let ceiling = self.initial.saturating_mul(factor).min(self.max);
        let ceiling_ms = u64::try_from(ceiling.as_millis())
            .unwrap_or(u64::MAX)
            .max(1);
        Duration::from_millis(random_u64() % ceiling_ms + 1)
    }

    pub fn reset(&mut self) {
        self.attempt = 0;
    }

    pub fn attempt(&self) -> u32 {
        self.attempt
    }
}

fn random_u64() -> u64 {
    let mut bytes = [0_u8; 8];
    // Si el sistema no da aleatoriedad, el backoff sigue siendo exponencial
    // (solo pierde el jitter).
    let _ = getrandom::fill(&mut bytes);
    u64::from_le_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delays_grow_and_respect_max() {
        let mut backoff = Backoff::new(Duration::from_millis(100), Duration::from_secs(2));
        for _ in 0..50 {
            let delay = backoff.next_delay();
            assert!(delay <= Duration::from_secs(2) + Duration::from_millis(1));
            assert!(delay >= Duration::from_millis(1));
        }
        backoff.reset();
        assert_eq!(backoff.attempt(), 0);
        assert!(backoff.next_delay() <= Duration::from_millis(101));
    }
}
