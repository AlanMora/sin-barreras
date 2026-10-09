# Runbook: el registro se atrasa o hay eventos en la cola de errores

## `RegistroConRetraso` / `StreamDelRegistroCreciendo`
- ¿Postgres responde? `docker service ps sin-barreras_postgres` → si no, ver
  `failover-postgres.md`.
- ¿record-writer vivo? `docker service ps sin-barreras_record-writer`.
- Mientras tanto nada se pierde: los eventos esperan en Valkey (AOF, fsync
  always). Vigilar la memoria de Valkey: con `noeviction`, si se llena, las
  escrituras fallan (el BFF deja de aceptar comandos y los ciudadanos ven
  el respaldo telefónico).

## `EventosEnColaDeErrores`
```bash
docker compose run --rm admin dlq list      # (en Swarm: el mismo comando con sb-admin)
```
Cada entrada trae su motivo. Causas típicas: versión de esquema nueva sin
migrar, dato fuera de rango. Corregir la causa y:
```bash
sb-admin dlq replay --all
```
La escritura es idempotente: reinyectar nunca duplica.

## `EventosTecnicosDescartados`
El BFF no pudo publicar eventos técnicos (conexiones, acks) porque Valkey no
respondía. Los eventos de contenido NO se pierden (son atómicos). Revisar Valkey.
