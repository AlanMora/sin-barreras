# 0012. Cliente `fred` para Valkey y `aws-lc-rs` como proveedor criptográfico

- **Estado:** Aceptado
- **Fecha:** 2026-10-09

## Contexto
El `CLAUDE.md` propone `fred` (o `redis-rs` con una razón clara). Necesitamos
TLS con verificación contra nuestra CA, reconexión automática, pub/sub con
re-suscripción, streams y scripts Lua.

## Decisión
- **`fred` 10**: soporta oficialmente Valkey; reconexión con backoff configurable;
  cliente suscriptor que se re-suscribe solo tras reconectar
  (`manage_subscriptions`); scripts con recarga automática si Valkey reinicia
  (`evalsha_with_reload`). No hay razón para cambiar a `redis-rs`.
- **TLS con `rustls`** (sin OpenSSL): imágenes distroless más simples y menos
  superficie de ataque.
- **Proveedor criptográfico único `aws-lc-rs`** (lo usan `fred` y `reqwest`):
  evita mezclar `ring` y `aws-lc-rs` y tiene validación FIPS 140-3 disponible.
- El usuario de Valkey del BFF tiene permiso `+info` (solo lectura), que `fred`
  usa para detectar la versión del servidor.

## Consecuencias
- Un solo stack TLS para Valkey, Postgres (sqlx con rustls, Fase 2) y HTTP.
