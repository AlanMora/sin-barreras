# 0018. Transporte HTTP de respaldo con la misma semántica que el WebSocket

- **Estado:** Aceptado
- **Fecha:** 2026-10-09

## Contexto
Algunas redes (proxies corporativos, Wi-Fi de hospitales y escuelas, ciertos
operadores) bloquean o cortan WebSocket. Si el único camino al C5 fuera el
WebSocket, esas personas no podrían pedir ayuda.

## Decisión
- El BFF expone `POST /api/v1/hello` y `POST /api/v1/sync` (long-polling de hasta
  25 s). Ambos usan **exactamente** el mismo código de aceptación durable,
  idempotencia, registro atómico y `seq` que el WebSocket (`session.rs`).
- La PWA tiene un `HttpSocket` con la interfaz de un WebSocket: el cliente de
  conexión no cambia. Tras 3 fallas seguidas al abrir el WebSocket se usa HTTP;
  cada 4.º intento (y cada 5 minutos) se vuelve a probar WebSocket.
- El latido en HTTP es el propio ciclo de long-polling; los `ping` se responden
  localmente.
- Un mensaje empezado por WebSocket y reenviado por HTTP no se duplica (probado).

## Consecuencias
- Más latencia en HTTP (hasta el siguiente ciclo), pero ningún pedido de ayuda
  queda sin camino.
- E2E: "red que bloquea WebSocket: la conversación sigue por HTTP".
