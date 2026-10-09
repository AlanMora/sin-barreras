# 0011. Entrega garantizada y ordenada de los mensajes del ciudadano al C5

- **Estado:** Aceptado
- **Fecha:** 2026-10-09

## Contexto
"Ningún 'necesito ayuda' se pierde." El BFF corre con 2+ réplicas sin estado;
cualquiera puede morir en cualquier momento, y el C5 puede estar lento o caído.
Además, los mensajes de una conversación deben llegar al operador en orden.

## Decisión
1. **Aceptación durable:** un script Lua atómico en Valkey verifica que el
   `client_msg_id` no se haya visto, agrega el comando al stream de entrada de la
   sesión (`sb:s:{sid}:in`), marca la sesión como pendiente (`sb:sweep`) y, en
   chats, emite el eco con su `seq`. Solo después se responde `ack`.
2. **Un forwarder por sesión:** el reenvío al C5 lo hace la réplica que obtiene
   el *lease* `sb:s:{sid}:fwd` (TTL 15 s, renovado antes de cada llamada). Procesa
   los comandos en orden y marca el avance (`fwd_id`) solo si sigue siendo dueña.
3. **Reintentos:** fallas temporales del C5 se reintentan indefinidamente con
   backoff exponencial y jitter (200 ms → 10 s). Los rechazos definitivos
   (4xx) se reportan al ciudadano como `failed`.
4. **Barrido:** cada réplica revisa cada 2 s las sesiones con pendientes de más
   de 3 s. Si la dueña murió, su lease expira y otra réplica continúa.
5. **Idempotencia en el C5:** cada llamada lleva el `client_msg_id` como llave
   de idempotencia. Si dos réplicas llegaran a reenviar lo mismo (un lease que
   expira durante una llamada lenta), el C5 no lo duplica.
6. **Eventos del C5 hacia el ciudadano:** todas las réplicas consumen el flujo
   del C5 y cada evento se emite una sola vez gracias a una llave de
   deduplicación (`sb:c5e:<event_id>`) dentro del mismo script atómico que
   asigna el `seq`.

## Alternativas consideradas
- Un único consumer group global de Valkey Streams: reparte los mensajes de
  una misma sesión entre réplicas y puede desordenarlos.
- Reenviar en memoria desde la conexión WebSocket: si la réplica muere, el
  mensaje se pierde.

## Consecuencias
- Probado: réplica que muere con un mensaje pendiente (otra lo entrega),
  reintentos en orden, duplicados por reconexión y entrega entre réplicas.
- Peor caso de demora tras la muerte de una réplica: TTL del lease (15 s) +
  intervalo de barrido (2 s).
- Requiere que el adaptador real del C5 acepte una llave de idempotencia
  (pregunta abierta en `docs/c5-preguntas.md`).
