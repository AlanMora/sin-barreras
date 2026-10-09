# 0005. TLS interno en todos los entornos, incluido desarrollo

- **Estado:** Aceptado
- **Fecha:** 2026-10-09

## Contexto
El proyecto exige TLS en todo el tráfico, incluido el interno hacia Postgres y
Valkey. Si el desarrollo local corre sin TLS, los problemas de certificados,
SNI o verificación aparecen por primera vez en producción.

## Decisión
- Valkey y Postgres aceptan **solo** TLS también en desarrollo.
  - Valkey: `port 0` + `tls-port 6379`.
  - Postgres: `hostssl` con SCRAM-SHA-256 y `hostnossl ... reject`; TLS 1.3
    como mínimo.
- `infra/dev/bootstrap.sh` crea una CA de desarrollo propia (ECDSA P-256),
  certificados de servidor con SAN y contraseñas aleatorias por servicio.
  Nada de esto se versiona.
- Los clientes Rust usarán `rustls` y verificarán el certificado contra la CA
  (`sslmode=verify-full` en Postgres).
- En Swarm la CA y las llaves llegan como Docker secrets (Fase 8).

## Consecuencias
- Paridad real entre desarrollo y producción.
- Un paso extra al clonar el repo (`make bootstrap`).
- mTLS (certificados de cliente) queda como evaluación para la Fase 8.
