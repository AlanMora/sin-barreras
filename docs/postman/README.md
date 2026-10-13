# Probar la plataforma con Postman

`sin-barreras.postman_collection.json` tiene **todos los endpoints HTTP** de la
plataforma en desarrollo local (51 peticiones), con pruebas automáticas en
cada una.

## Preparación

1. Levanta el sistema:

   ```bash
   make up-apps
   # Para la carpeta 07 (métricas), agrega el perfil de observabilidad:
   docker compose --profile apps --profile observability up -d --build --wait
   ```

2. En Postman: **Import** → elige `docs/postman/sin-barreras.postman_collection.json`.
3. Abre la colección → pestaña **Variables** y llena estas dos (se generan en
   cada máquina con `make bootstrap`; no están en Git):

   | Variable | De dónde sale |
   |---|---|
   | `c5_api_key` | `cat infra/dev/secrets/c5_mock_api_key` |
   | `qr_signature` | `docker compose --profile apps exec bff /app sign-qr TOT-0142 1 http://localhost:8000` (lo que va después de `s=`) |

   Las demás variables (`session_id`, `session_token`, `last_seq`,
   `incident_id`, `c5_incident_id`) se llenan solas mientras usas la colección.

4. Abre la **Console** de Postman (abajo a la izquierda). Ahí cada respuesta
   del BFF aparece resumida y legible, por ejemplo:

   ```
   seq 7 · chat_message · operator: "Te atiende el C5 Zapopan…"
   seq 9 · incident_status · unit_en_route (≈ 8 min)
   ```

## Recorrido sugerido

1. **01 · Salud**: confirma que todo está arriba.
2. **02 · Flujo del ciudadano**: ejecuta los pasos **en orden**, del 1 al 13.
   Repite el paso 7 varias veces para ver llegar las respuestas del operador.
3. **05 · Controles del C5**: ejecuta *Listar incidentes* y después haz de
   operador (escribe mensajes, cambia el estado). Vuelve a **02 → 7** para ver
   cómo le llega al ciudadano.
4. Abre http://localhost:8000 en otra pestaña si quieres ver la PWA al mismo
   tiempo. Ten en cuenta que es otra sesión, distinta a la de Postman.

Para correr la colección completa desde la terminal, sin instalar nada en el
proyecto:

```bash
pnpm dlx newman@6 run docs/postman/sin-barreras.postman_collection.json \
  --env-var "c5_api_key=$(cat infra/dev/secrets/c5_mock_api_key)" \
  --env-var "qr_signature=<tu firma>"
```

## Inventario de endpoints

### Lo que usa el ciudadano (entra por Traefik, `localhost:8000`)

| Método | Ruta | Servicio | Para qué |
|---|---|---|---|
| GET | `/` y cualquier ruta de la app | Nginx | La PWA (`/chat`, `/silencioso`, `/mis-reportes`…) |
| GET | `/healthz` | Nginx | Salud de Nginx |
| GET | `/ws` | BFF | **WebSocket**: el canal principal de la PWA |
| POST | `/api/v1/hello` | BFF | Abrir o reanudar sesión (respaldo HTTP del WebSocket) |
| POST | `/api/v1/sync` | BFF | Enviar comandos y recibir mensajes nuevos (respaldo HTTP) |

`/api/v1/hello` y `/api/v1/sync` llevan exactamente el mismo protocolo que el
WebSocket. Existen para redes que bloquean WebSocket (ADR-0018), y la PWA cambia
a ellos sola cuando hace falta. Postman los usa porque se pueden probar uno por uno.

### Internos de cada servicio (solo en tu máquina)

En producción no se pueden alcanzar desde fuera. En desarrollo cada réplica
tiene su puerto:

| Servicio | Réplica 1 | Réplica 2 | Rutas |
|---|---|---|---|
| BFF | `localhost:18080` | `localhost:18081` | `/health`, `/ready`, `/metrics` |
| record-writer | `localhost:18181` | `localhost:18182` | `/health`, `/ready`, `/metrics` |
| c5-mock | `localhost:8090` | — | `/health`, `/ready` |

### Simulador del C5 (`localhost:8090`) — API inventada

| Método | Ruta | Para qué |
|---|---|---|
| POST | `/mock-api/v1/incidents` | Crear incidente (lo usa el BFF) |
| POST | `/mock-api/v1/incidents/{id}/messages` | Mensaje del ciudadano al C5 |
| POST | `/mock-api/v1/incidents/{id}/locations` | Ubicación al C5 |
| POST | `/mock-api/v1/incidents/{id}/video-calls` | Pedir videollamada |
| GET | `/mock-api/v1/events?since=N` | **WebSocket**: eventos del C5 hacia el BFF |
| GET | `/mock-control/incidents` | Ver todos los incidentes simulados |
| POST | `/mock-control/incidents/{id}/operator-message` | Hacer de operador: escribir |
| POST | `/mock-control/incidents/{id}/status` | Hacer de operador: cambiar estado |
| GET / POST | `/mock-control/chaos` | Ver o activar latencia y fallas simuladas |

Las rutas `/mock-api/*` piden la cabecera `x-api-key`. Cuando llegue la
documentación del C5 real, todo esto se reemplaza por un adaptador nuevo.

### Observabilidad (perfil `observability`)

| Servicio | Dirección |
|---|---|
| Prometheus | http://localhost:9091 |
| Grafana | http://localhost:3300 |

### Lo que no es HTTP

| Servicio | Dirección | Cómo se usa |
|---|---|---|
| Postgres primario | `localhost:15432` | Cliente SQL con TLS; los datos sensibles están cifrados |
| Postgres réplica | `localhost:15433` | Solo lectura |
| Valkey | `localhost:16379` | Cliente Redis con TLS |
| Herramientas de auditoría | — | `make verify`, `make dlq`, `make investigate` |

## Probar el WebSocket en Postman

Postman puede abrir WebSockets, pero el formato de colección que se exporta no
los guarda. Por eso no están en la colección. Para probarlo a mano:

1. **New → WebSocket**. URL: `ws://localhost:8000/ws` → **Connect**.
2. Tienes 10 segundos para enviar el `hello`, o el BFF cierra con el código `4408`:

   ```json
   {"type":"hello","client_msg_id":"6f1c2c1e-2a3b-4c5d-8e9f-001122334455","protocol_version":1,"app_version":"postman","language":"es-MX"}
   ```

3. Envía un mensaje. Cada comando necesita un `client_msg_id` **nuevo**; si
   repites uno, el BFF responde `duplicate`:

   ```json
   {"type":"chat_message","client_msg_id":"6f1c2c1e-2a3b-4c5d-8e9f-001122334466","kind":"text","text":"Necesito ayuda"}
   ```

4. Las respuestas del operador y los cambios de estado llegan solos. Si no
   mandas nada en 45 s, el BFF cierra la conexión (`4409`). Para mantenerla
   abierta, envía un `ping` de vez en cuando:

   ```json
   {"type":"ping","client_msg_id":"6f1c2c1e-2a3b-4c5d-8e9f-001122334477"}
   ```

Si Postman recibe `403`, quita la cabecera `Origin` o pon `http://localhost:8000`.

Todos los mensajes y sus campos están en `docs/contrato-eventos.md`.
