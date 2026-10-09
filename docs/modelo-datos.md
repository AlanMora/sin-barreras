# Modelo de datos del registro completo

El registro es el respaldo legal y operativo del lado ciudadano: **todo** lo
que pasa por el BFF, en ambas direcciones, queda guardado de forma permanente,
cifrada, de solo inserción y encadenada.

```
BFF ──(script Lua atómico)──▶ Valkey stream sb:rec:events (AOF, fsync always)
                                   │ consumer group "record-writers"
                                   ▼
                       record-writer (2 réplicas) ──▶ Postgres primario ──▶ réplica
                                   │ fallas repetidas
                                   ▼
                          sb:rec:dlq (cola de errores)
```

## Esquemas

| Esquema | Contenido | Quién accede |
|---|---|---|
| `registry` | Registros, llaves de sesión, checkpoints | `sb_writer` (insertar), `sb_auditor` (leer), `sb_investigator` (Fase 7, vía funciones auditadas) |
| `reporting` | Vistas sin contenido sensible | `sb_reporting` (en la réplica) |
| `sb_migrations` | Control de migraciones de sqlx | `sb_owner` (vía `sb_migrator`) |

## `registry.records`

Una fila por evento. Nunca se modifica ni se borra (permisos + triggers).

| Columna | Tipo | Descripción |
|---|---|---|
| `record_id` | bigint identity | Orden de persistencia; lo usan los checkpoints |
| `event_id` | uuid único | UUIDv7 del evento; llave de idempotencia |
| `schema_version` | smallint | Versión del sobre de eventos |
| `chain_id` | text | Cadena de integridad: el `session_id`, o `system-AAAA-MM-DD` |
| `chain_seq` | bigint | Posición en la cadena (1, 2, 3… sin huecos) |
| `kind` | text | Tipo de evento (ver catálogo) |
| `occurred_at` | timestamptz | Hora del servidor (BFF), UTC, milisegundos |
| `recorded_at` | timestamptz | Hora de escritura en Postgres |
| `client_ts` | timestamptz | Hora del dispositivo, si aplica |
| `session_id`, `incident_id` | uuid | Nuestros identificadores |
| `c5_incident_id` | text | Identificador del C5 (cruce de registros) |
| `producer` | text | `bff/<réplica>` |
| `trace_id` | text | Traza W3C para correlacionar con logs y trazas |
| `stream_id` | text | Id de la entrada en Valkey |
| `attributes_json` / `attributes` | text / jsonb | Datos **no sensibles** (JSON canónico, y su versión consultable) |
| `sealed_alg`, `sealed_key_ref`, `sealed_nonce`, `sealed_ciphertext` | | Contenido **sensible cifrado** (AES-256-GCM) |
| `prev_hash`, `hash` | bytea(32) | Encadenamiento SHA-256 |

### Qué es sensible y va cifrado

| Evento | Contenido cifrado |
|---|---|
| `citizen_message`, `operator_message` | Texto completo del mensaje (incluye respuestas rápidas) |
| `location_received` | Latitud, longitud, precisión, fuente, hora de captura |
| `client_metadata` | IP y user agent |
| `video_call_updated` | Datos de la sala y referencias de la videollamada |

Todo lo demás (tipos, estados, canal, ids, versiones, ajustes de
accesibilidad, punto de entrada) va en `attributes` sin cifrar, para poder
hacer métricas sin descifrar nada.

## Catálogo de eventos

| Grupo | Eventos |
|---|---|
| Sesión | `session_key_created`, `session_started`, `session_resumed`, `session_resume_rejected`, `session_closed`, `connection_opened`, `connection_closed`, `client_metadata`, `entry_point` |
| Interfaz | `client_event` (`app_opened`, `privacy_notice_shown`, `channel_screen_shown`, `abandoned_before_help`, `quick_exit_used`, `settings_changed`, `permission_*`, `went_offline`, `came_back_online`…) |
| Incidente | `incident_requested`, `incident_created` (con `c5_incident_id`), `incident_status_changed`, `first_operator_response` (con `latency_ms`) |
| Mensajes | `citizen_message`, `citizen_message_forwarded`, `citizen_message_failed`, `operator_message`, `message_status_changed`, `client_ack` |
| Ubicación | `location_received`, `location_forwarded` |
| Videollamada | `video_call_requested`, `video_call_updated` |
| Anomalías | `protocol_error`, `fields_dropped`, `c5_call_failed`, `c5_orphan_event` |

## `registry.session_keys`

La llave de datos (DEK) de cada sesión, envuelta con HPKE para la llave pública
de investigación (ADR-0014). Sin la llave **privada** de investigación, nadie
puede descifrar nada; sin la fila de la sesión, esa sesión queda ilegible para
siempre (borrado criptográfico, ADR-0017).

## `registry.checkpoints`

Cada cierto tiempo (60 s en desarrollo, configurable) se crea un checkpoint con
la raíz SHA-256 de todos los registros nuevos (por `record_id`), encadenado al
checkpoint anterior.

## Integridad: qué detecta `sb-admin verify`

| Ataque | Cómo se detecta |
|---|---|
| Alterar el contenido, el texto cifrado o cualquier columna | El hash recalculado no coincide |
| Borrar un registro intermedio | `chain_seq` con hueco y `prev_hash` roto |
| Reordenar registros | `prev_hash` roto |
| Borrar una sesión completa | La raíz del checkpoint no coincide |
| Alterar y recalcular hashes de toda una cadena | La raíz del checkpoint no coincide |
| Borrar o alterar un checkpoint | La cadena de checkpoints se rompe |

Recomendación (Fase 8): publicar o sellar externamente el hash de cada
checkpoint (correo a jurídico, sello de tiempo RFC 3161) para que ni siquiera
un superusuario de la base pueda reescribir la historia sin ser detectado.

## Garantías de no pérdida (probadas)

| Escenario | Prueba |
|---|---|
| Postgres caído | `postgres_down_events_wait_in_the_stream_and_nothing_goes_to_dead_letter` |
| record-writer muerto a media escritura | `writer_killed_mid_batch_is_recovered_by_another_without_duplicates` |
| Valkey reiniciado (antes y durante) | `valkey_restart_keeps_unsaved_events` |
| Eventos inválidos | `poison_events_go_to_dead_letter_without_blocking_the_rest` |
| Duplicados por reconexión y 2 réplicas del BFF | `each_record_is_written_exactly_once_even_with_duplicates_and_two_replicas` |
| Alteración de datos | `verify_detects_every_kind_of_tampering`, `checkpoints_detect_deletion_of_a_whole_session…` |

## Riesgos residuales

- **Valkey guarda en claro el estado operativo** de las sesiones activas (los
  mensajes pendientes de reenviar y el historial reciente para reconexión),
  con TTL de 30 min a 12 h. El registro permanente sí va cifrado. Mitigación:
  TLS, ACL por servicio y volumen de Valkey en disco cifrado (Fase 8).
- Los eventos técnicos que se registran de forma asíncrona (conexiones, acks,
  errores) podrían perderse si una réplica del BFF muere en los ~100 ms entre
  que ocurren y se publican. El contenido de la conversación y los cambios de
  estado NO tienen esa ventana: se registran de forma atómica.
