# 0009. Imágenes mínimas con healthcheck integrado en el binario

- **Estado:** Aceptado
- **Fecha:** 2026-10-09

## Contexto
Las imágenes de producción deben ser mínimas (menos superficie de ataque), pero
Swarm necesita un `HEALTHCHECK` para reemplazar contenedores enfermos y para
hacer rollback automático en un despliegue fallido.

## Decisión
- Servicios Rust: build multi-stage con `rust:1.99.0-slim-trixie` e imagen
  final `gcr.io/distroless/cc-debian13:nonroot` (glibc, sin shell, usuario
  65532). Tamaño actual: ~46 MB.
- Como distroless no tiene `curl`, cada binario acepta el subcomando
  `healthcheck` (`service_kit::health::probe`), que consulta su propio
  `/health` por TCP sin dependencias extra.
- `/health` = proceso vivo (no consulta dependencias, para evitar reinicios en
  cascada). `/ready` = dependencias accesibles. Ninguno se publica por Traefik.
- En compose y Swarm: `read_only: true`, `cap_drop: [ALL]`,
  `no-new-privileges`. Valkey y Postgres conservan solo las capacidades
  necesarias para copiar su material TLS y bajar a su usuario.

## Consecuencias
- Sin shell en los contenedores Rust: para depurar se usa
  `docker run --pid=container:<id>` con una imagen de herramientas.
- Apagado ordenado verificado: SIGTERM → drenado → salida con código 0.
