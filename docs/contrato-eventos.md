# Contrato PWA ↔ BFF (protocolo v1)

Protocolo de mensajes JSON sobre WebSocket entre la PWA ciudadana y el BFF.

- **Fuente de verdad:** los tipos de `crates/protocol`. Los tipos TypeScript de
  la PWA (`apps/pwa/src/protocol/generated.ts`) se generan de ahí y una prueba
  falla si se desincronizan.
- **Endpoint:** `wss://<dominio>/ws` (Traefik → BFF).
- **Tamaño máximo de un frame del cliente:** 16 KB. **Texto de chat:** 2000 caracteres.
- Todas las marcas de tiempo son RFC 3339 en UTC con milisegundos: `2026-10-09T18:37:19.842Z`.

## Principios

| Regla | Cómo se cumple |
|---|---|
| Idempotencia | Todo mensaje del cliente lleva `client_msg_id` (UUID). Si se reenvía por reconexión, el BFF responde `ack` con `status: "duplicate"` y no lo procesa de nuevo. |
| Orden y recuperación | Todo mensaje del servidor que forma parte de la conversación lleva `seq` (1, 2, 3… por sesión, sin huecos). Al reconectar, el cliente envía su último `seq` y el BFF reenvía lo posterior. |
| ACK en ambas direcciones | El servidor confirma cada comando con `ack`. El cliente confirma cada mensaje secuenciado con `ack { seq }`. |
| Durabilidad | El `ack` del servidor significa "guardado de forma durable en Valkey"; a partir de ahí no se pierde aunque el BFF o el C5 fallen. Si el BFF no puede guardarlo, responde `error` con `retryable: true` y el cliente lo conserva en su cola. |
| Versión | `hello.protocol_version`. Si no es compatible: `error` `unsupported_protocol_version` con `supported_versions` y cierre `4426`. |
| Validación | Campos desconocidos se rechazan. Un campo **opcional** inválido se descarta y se registra, sin rechazar el mensaje (ADR-0007). |

## Ciclo de vida

```
PWA                                         BFF
 │── hello {protocol_version, resume?, entry?} ─▶│  (≤10 s tras abrir)
 │◀──────── welcome {session_id, session_token, latest_seq, incident?}
 │◀──────── [mensajes secuenciados faltantes: seq > last_seq]
 │── create_incident {channel} ─────────────────▶│
 │◀──────── ack {status: accepted}                │ (durable en Valkey)
 │◀──────── incident_created {seq: 1}             │ (C5 confirmó)
 │◀──────── incident_status {seq: 2, received}    │
 │── chat_message {text} ───────────────────────▶│
 │◀──────── ack {accepted}                        │  → "enviado" ✓
 │◀──────── chat_message {seq: 3, author: citizen}│  (eco para el historial)
 │◀──────── message_status {seq: 4, delivered}    │  → "entregado" ✓✓
 │◀──────── message_status {seq: 5, read}         │  → "leído"
 │◀──────── chat_message {seq: 6, author: operator}
 │── ack {seq: 6} ──────────────────────────────▶│
 │── ping (cada 15 s) ──────────────────────────▶│
 │◀──────── pong                                  │
```

## Mensajes del cliente

Campos comunes a todos: `type`, `client_msg_id` (UUID, obligatorio),
`client_ts` (opcional), `traceparent` (opcional, W3C).

| `type` | Campos | Notas |
|---|---|---|
| `hello` | `protocol_version`, `app_version`, `language` (BCP 47), `resume?` `{session_id, session_token, last_seq}`, `entry?` `{point_id, key_id?, signature}`, `accessibility?` `{font_scale, high_contrast, reduced_motion, silent_mode}` | Primer mensaje obligatorio. |
| `create_incident` | `channel`: `lsm_video` \| `text_chat` \| `audio` \| `translation` \| `silent`; `language?` | Idempotente: si ya existe incidente, no crea otro. |
| `chat_message` | `kind`: `text` \| `quick_reply`; `text`; `quick_reply?`: `in_danger` \| `cannot_speak` \| `being_followed` \| `injured` | Si aún no hay incidente, **lo crea automáticamente** (nunca se bloquea un pedido de ayuda). |
| `location_update` | `latitude`, `longitude`, `accuracy_m?`, `source`: `gps` \| `network`, `captured_at` | Antes de existir el incidente se guarda la más reciente y se envía al crearlo. |
| `video_call_request` | — | Crea el incidente (canal `lsm_video`) si no existe. |
| `client_event` | `kind` (ver abajo), `detail?` (≤200 caracteres, no sensible) | Solo para el registro; no va al C5. |
| `ack` | `seq` | Confirma un mensaje secuenciado. No recibe respuesta. |
| `ping` | — | Cada 15 s. Responde `pong`. Renueva la sesión. |
| `close` | `reason`: `ended_by_citizen` \| `abandoned_before_help` | Cierra la sesión (no la salida rápida del modo silencioso). |

`client_event.kind`: `app_opened`, `privacy_notice_shown`, `privacy_notice_opened`,
`channel_screen_shown`, `abandoned_before_help`, `quick_exit_used`,
`settings_changed`, `permission_granted`, `permission_denied`, `went_offline`,
`came_back_online`, `client_error`.

## Mensajes del servidor

### Control (sin `seq`, no se reenvían)

| `type` | Campos |
|---|---|
| `welcome` | `protocol_version`, `session_id`, `session_token` (nuevo en cada welcome), `token_expires_at`, `resumed`, `entry`: `none` \| `verified` \| `unverified`, `heartbeat_interval_ms`, `latest_seq`, `incident?` `{incident_id, channel, status}`, `server_ts` |
| `ack` | `client_msg_id`, `status`: `accepted` \| `duplicate`, `server_ts` |
| `pong` | `client_msg_id`, `server_ts` |
| `error` | `code`, `message` (técnico; la PWA traduce el `code`), `client_msg_id?`, `retryable`, `supported_versions?`, `server_ts` |

Códigos de error: `unsupported_protocol_version`, `hello_required`,
`invalid_message`, `message_too_large`, `rate_limited`, `internal`.

### Secuenciados (con `seq`, se reenvían al reconectar)

| `type` | Campos |
|---|---|
| `incident_created` | `incident_id` (nuestro), `channel`, `client_msg_id` (lo que lo provocó) |
| `incident_status` | `status`: `received` \| `attending` \| `unit_en_route` \| `unit_on_scene` \| `closed`; `eta_seconds?` |
| `chat_message` | `message_id`, `author`: `citizen` \| `operator` \| `system`, `kind`, `text`, `quick_reply?`, `client_msg_id?` (en ecos del ciudadano) |
| `message_status` | `client_msg_id`, `status`: `delivered` \| `read` \| `failed` |
| `video_call` | `request_client_msg_id`, `status`: `requested` \| `ready` \| `failed` \| `ended`, `join?` (objeto opaco para el componente de video del otro equipo) |
| `session_closed` | `reason`: `incident_closed` \| `ended_by_citizen` \| `expired` |

Todos llevan además `seq` y `server_ts`.

## Reconexión

1. El cliente guarda `session_id`, `session_token` y el mayor `seq` **contiguo** recibido.
2. Al reconectar envía `hello` con `resume`. Si el token es válido, `welcome.resumed = true`
   y el token anterior queda invalidado (se rota en cada `welcome`).
3. El BFF reenvía todos los mensajes con `seq > last_seq`, en orden.
4. El cliente reenvía todo comando que no haya recibido `ack` **con el mismo `client_msg_id`**.
5. Si la sesión expiró o el token no es válido, se crea una sesión nueva (`resumed = false`)
   y el cliente reenvía sus pendientes en ella.

La sesión vive mientras haya actividad: 30 min sin incidente; 12 h con un
incidente abierto (configurable, ADR-0010).

## Códigos de cierre

| Código | Motivo |
|---|---|
| 4400 | `hello` inválido |
| 4408 | No llegó `hello` en 10 s |
| 4409 | Sin actividad del cliente en 45 s |
| 4410 | La sesión expiró con la conexión abierta: hacer `hello` de nuevo |
| 4426 | Versión de protocolo no soportada |
| 4503 | Servicio no disponible temporalmente: reintentar con backoff |

## Videollamada (punto de integración)

La PWA envía `video_call_request`. Cuando el sistema de videollamadas del otro
equipo prepara la sala, llega `video_call { status: "ready", join: {...} }`. El
objeto `join` es **opaco**: la PWA lo entrega tal cual al componente de video
(ver `apps/pwa/src/video/`). El BFF no lo interpreta; solo lo registra cifrado.
