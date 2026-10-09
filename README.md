# Sin Barreras

Parte ciudadana del **Programa de Atención Incluyente de Emergencias** del
Gobierno de Zapopan, operado por el C5. Permite pedir ayuda sin hacer una
llamada: personas sordas (LSM), con discapacidad visual o motriz, personas
mayores, niñas y niños, personas que no pueden hablar y personas extranjeras.

```
Celular → Traefik (TLS) → Nginx (PWA estática)
                        → BFF Rust (WebSocket + REST) → C5 (existente)
                                   ↓
                                 Valkey (sesiones, rate limit, pub/sub, Streams)
                                   ↓
                          record-writer Rust → Postgres primario → réplica
```

## Estructura

| Ruta | Contenido |
|---|---|
| `apps/pwa` | PWA ciudadana (Preact + Vite + Tailwind) |
| `apps/bff` | Backend For Frontend (Rust, axum) |
| `apps/record-writer` | Registro permanente en Postgres (Rust) |
| `apps/c5-mock` | Simulador del C5 para desarrollo (**API inventada**) |
| `crates/` | Código Rust compartido (ver ADR-0002) |
| `infra/` | Dockerfiles, configuración de Valkey, Postgres y Nginx |
| `docs/` | ADRs, contrato de eventos, modelo de datos, preguntas al C5 |

## Requisitos

- Rust (se instala solo con `rustup` según `rust-toolchain.toml`)
- Node 24 y pnpm 11 (`corepack enable`)
- Docker con Compose v2
- OpenSSL (para generar los certificados de desarrollo)

## Primeros pasos

```bash
make bootstrap   # certificados TLS y secretos de desarrollo + pnpm install
make up          # Valkey + Postgres (solo TLS)
make check       # lint + tests + build, igual que el CI
make up-apps     # todo el sistema en contenedores
```

| Servicio | URL local |
|---|---|
| PWA | http://localhost:8000 |
| BFF | http://localhost:8080/health |
| record-writer | http://localhost:8081/health |
| c5-mock | http://localhost:8090/health |
| Postgres | `localhost:15432` (TLS obligatorio) |
| Valkey | `localhost:16379` (TLS obligatorio) |

Los puertos del host se cambian con `.env` (ver `.env.example`).

Desarrollo de la PWA con recarga en caliente: `pnpm --filter @sin-barreras/pwa dev`.

## Reglas que no se negocian

1. Guardar nunca bloquea la atención de una emergencia.
2. No se pierde ningún registro.
3. Ninguna validación bloquea la petición de ayuda (ADR-0007).
4. Los logs nunca contienen mensajes, ubicación ni IP.
