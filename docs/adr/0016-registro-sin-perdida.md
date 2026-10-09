# 0016. Registro sin pérdida: escritura atómica, consumer groups e idempotencia

- **Estado:** Aceptado
- **Fecha:** 2026-10-09

## Contexto
Reglas de oro: (1) guardar nunca bloquea la emergencia; (2) no se pierde
ningún registro. Además el registro debe tener valor probatorio.

## Decisión
1. **Escritura atómica en el BFF:** los eventos de contenido y de estado se
   agregan a `sb:rec:events` dentro del mismo script Lua que acepta el comando
   o emite el mensaje al ciudadano. El `seq` que vio el ciudadano se inyecta en
   el registro dentro del script. No existe un instante en que el mensaje
   exista y su registro no. Eventos técnicos (conexiones, acks): canal asíncrono
   acotado que nunca bloquea.
2. **record-writer** (2+ réplicas) consume con el consumer group
   `record-writers`, creado desde el inicio del stream (`0`).
3. **XACK solo después del COMMIT.** Lote completo en una transacción.
4. **Idempotencia:** si el `event_id` ya existe, se omite. Reprocesar tras una
   caída entre COMMIT y XACK no duplica nada.
5. **Postgres caído:** se reintenta el mismo lote con backoff indefinidamente.
   El writer renueva la posesión de sus pendientes (`XCLAIM … JUSTID`, que no
   cuenta como entrega), así una caída larga **nunca** manda eventos a la cola
   de errores.
6. **Writer muerto:** sus pendientes quedan inactivos y otro writer los reclama
   (`XPENDING` con `IDLE` + `XCLAIM`).
7. **Cola de errores** `sb:rec:dlq`: entradas ilegibles, eventos con contenido
   sin cifrar, eventos que violan restricciones de datos, o que exceden el
   máximo de entregas (5). Primero se escribe en la cola y después se hace XACK.
   `sb-admin dlq replay` los reinyecta tras corregir la causa. Un error de
   permisos o de esquema (42xxx) **no** es permanente: se reintenta, para que
   una mala configuración no vacíe el stream hacia la cola de errores.
8. **Encadenamiento:** una cadena por sesión (candado por cadena, tomado en orden
   para evitar interbloqueos) y checkpoints globales que encadenan todos los
   registros (candado global compartido/exclusivo para no saltar registros a
   medio escribir).

## Consecuencias
- Probado con Valkey y Postgres reales: Postgres caído, writer muerto a media
  escritura, Valkey reiniciado antes y durante el procesamiento, eventos
  venenosos, dos writers en paralelo. En todos los casos: exactamente una vez.
- El orden dentro de una cadena es el orden de persistencia. El orden causal se
  reconstruye con `occurred_at` y el `seq` registrado.
