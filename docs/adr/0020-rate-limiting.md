# 0020. Rate limiting que degrada y nunca bloquea el pedido de ayuda

- **Estado:** Aceptado
- **Fecha:** 2026-10-09

## Contexto
El BFF es público. Hay que protegerlo del abuso sin dejar sin ayuda a nadie:
con CGNAT, miles de teléfonos comparten una IP pública, y en un desastre
(sismo, inundación, evento masivo) todos piden ayuda a la vez desde la misma red.

## Decisión
Contadores de ventana fija (60 s) en Valkey, atómicos (Lua), **que fallan
abiertos** (si Valkey no responde, se permite).

| Ámbito | Umbral por defecto | Al excederse | Variable |
|---|---|---|---|
| `create_incident`, `hello` reanudado, `ack`, `ping`, `close` | **Sin límite** | — | — |
| Primeros mensajes de chat de la sesión | **5 libres** | — | `RATE_FREE_MESSAGES` |
| Chat por sesión | 30/min | `rate_limited` **reintentable**: el mensaje sigue en la cola del ciudadano y se envía al abrirse la ventana | `RATE_CHAT_PER_MIN` |
| Ubicación por sesión | 20/min | ídem | `RATE_LOCATION_PER_MIN` |
| Eventos de interfaz por sesión | 60/min | ídem | `RATE_CLIENT_EVENTS_PER_MIN` |
| Solicitudes de llamada por sesión | 6/min | ídem | `RATE_CALL_REQUESTS_PER_MIN` |
| Sesiones nuevas por IP | 120/min → **solo se registra** (`rate_limited`, `scope=ip`, `action=observe`) | Se permite | `RATE_IP_OBSERVE_PER_MIN` |
| Sesiones nuevas por IP (límite duro) | **Desactivado** (0) | `4429` / HTTP 429 reintentable | `RATE_IP_HARD_PER_MIN` |

- El límite duro por IP solo se activa de forma deliberada durante un ataque en
  curso, sabiendo que puede afectar a usuarios legítimos detrás de CGNAT.
- Cada limitación se registra (`rate_limited`) sin la IP en claro.
- Defensa adicional en el borde (Fase 8): límites de conexión de Traefik por
  IP, holgados.

## Consecuencias
- Un atacante con una sola sesión no puede inundar al operador.
- Un ataque distribuido de creación de sesiones no se frena por IP en modo
  normal; se detecta (alertas) y se decide activar el límite duro.
