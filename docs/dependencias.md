# Dependencias y su justificación

Regla del proyecto: no se agrega ninguna dependencia sin justificarla aquí.
Se prefieren crates/paquetes mantenidos, ampliamente usados y con pocas
dependencias transitivas.

## Rust

| Crate | Dónde | Para qué | Por qué esta y no otra |
|---|---|---|---|
| `tokio` | todos | runtime asíncrono | estándar de facto; lo exige axum |
| `axum` (+`ws`) | servicios | HTTP y WebSocket | indicado en el proyecto; WebSocket nativo |
| `tower`, `tower-http` | servicios | middleware | ecosistema de axum |
| `serde`, `serde_json` | todos | JSON | estándar |
| `thiserror`, `anyhow` | todos | errores tipados / binarios | indicado en el proyecto |
| `tracing`, `tracing-subscriber` | todos | logs estructurados | indicado en el proyecto |
| `uuid` (v4, v5, v7) | todos | ids; v7 ordenable por tiempo, v5 ids estables derivados | estándar |
| `time` | protocol | marcas RFC 3339 en ms | más pequeño y sin las fallas históricas de `chrono` |
| `ts-rs` | protocol | generar los tipos TypeScript del contrato | genera desde los mismos tipos serde; evita mantener dos definiciones |
| `fred` | bff, record-writer | cliente de Valkey | ver ADR-0012 |
| `reqwest` (rustls) | bff | HTTP hacia el C5 | estándar; TLS sin OpenSSL |
| `tokio-tungstenite` | bff | feed WebSocket del C5 simulado; cliente en pruebas | el mismo que usa axum por dentro |
| `futures` | bff | streams | estándar |
| `sha2`, `hmac`, `subtle` | bff | hash del token, firma HMAC de QR | RustCrypto, auditados |
| `base64` | bff | codificación base64url | estándar |
| `getrandom` | bff, c5-mock | aleatoriedad del sistema operativo para tokens y jitter | mínima; evita traer `rand` completo |
| `hpke` | crypto | envolver la DEK hacia la llave pública de investigación (RFC 9180) | implementación del estándar, sin criptografía hecha a mano |
| `aes-gcm` | crypto | AES-256-GCM del contenido | RustCrypto; la misma versión que ya usa `hpke` |
| `zeroize` | crypto, bff | borrar llaves y texto en claro de la memoria | estándar de RustCrypto |
| `sqlx` (rustls) | registry, record-writer, sb-admin | Postgres asíncrono + migraciones versionadas | indicado en el proyecto; TLS con el mismo stack |
| `testcontainers-modules` | pruebas | Valkey/Postgres reales en pruebas de integración | exigido: pruebas con servicios reales |

## PWA (TypeScript)

| Paquete | Para qué | Por qué |
|---|---|---|
| `preact`, `@preact/signals`, `preact-iso` | UI, estado, rutas | indicados en el proyecto; ~10 KB en total |
| `vite`, `@preact/preset-vite`, `@babel/core` | build | indicados; Babel lo exige el preset |
| `vite-plugin-pwa`, `workbox-window` | service worker | indicado |
| `tailwindcss`, `@tailwindcss/vite` | estilos con tokens propios | indicado |
| `typescript` | tipos estrictos | indicado |
| `vitest` | pruebas unitarias | nativo de Vite, sin configuración extra |
| `@biomejs/biome` | lint y formato | reemplaza ESLint + Prettier con una sola dependencia (ADR-0006) |
