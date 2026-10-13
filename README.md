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
- GNU Make (en Ubuntu/Debian: `sudo apt install make`)

## Primeros pasos

Los comandos `make <objetivo>` son atajos definidos en el `Makefile`: cada uno
solo encadena comandos de `cargo`, `pnpm` y `docker compose`. `make help`
lista todos.

```bash
make bootstrap   # una vez: certificados TLS y secretos de desarrollo + pnpm install
make up-apps     # todo el sistema en contenedores (reconstruye si cambió el código)
make smoke       # prueba de punta a punta contra el sistema levantado
```

`make up-apps` ya ejecuta `bootstrap`, así que en una máquina nueva basta con
él. Para levantar solo Valkey y Postgres (por ejemplo, para correr las pruebas
de Rust): `make up`.

Para agregar Grafana, Prometheus y Tempo:

```bash
docker compose --profile apps --profile observability up -d --build --wait
```

| Servicio | URL local |
|---|---|
| PWA | http://localhost:8000 |
| BFF | http://localhost:8000/api/health (a través de Traefik) |
| c5-mock | http://localhost:8090/health |
| Grafana | http://localhost:3300 (perfil `observability`) |
| Prometheus | http://localhost:9091 (perfil `observability`) |
| Postgres primario | `localhost:15432` (TLS obligatorio) |
| Postgres réplica | `localhost:15433` (TLS obligatorio) |
| Valkey | `localhost:16379` (TLS obligatorio) |

El BFF y `record-writer` no publican puertos en el host: el BFF solo se alcanza
por Traefik y `record-writer` no recibe tráfico. Su estado se ve con
`docker compose --profile apps ps`.

Los puertos del host se cambian con `.env` (ver `.env.example`).

Para probar la entrada por QR, genera una URL firmada con la llave de desarrollo:

```bash
docker compose --profile apps exec bff /app sign-qr TOT-0142 1 http://localhost:8000
```

Otros comandos frecuentes:

| Comando | Para qué |
|---|---|
| `make logs` | Sigue los logs de todos los servicios |
| `make down` | Detiene el sistema y conserva los datos (no detiene el perfil `observability`) |
| `make check` | Lint, pruebas y build, igual que el CI |
| `make e2e` | Pruebas E2E con Playwright y axe contra el sistema levantado |
| `make verify` | Verifica la cadena de integridad del registro |
| `make dlq` | Muestra la cola de errores del registro |

Desarrollo de la PWA con recarga en caliente: `pnpm --filter @sin-barreras/pwa dev`.

## Reglas que no se negocian

1. Guardar nunca bloquea la atención de una emergencia.
2. No se pierde ningún registro.
3. Ninguna validación bloquea la petición de ayuda (ADR-0007).
4. Los logs nunca contienen mensajes, ubicación ni IP.
