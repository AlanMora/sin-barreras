# 0022. Respaldos con pgBackRest, réplica de streaming y prueba de restauración automatizada

- **Estado:** Aceptado
- **Fecha:** 2026-10-09

## Decisión
- Imagen propia `postgres:18-alpine` + pgBackRest 2.58.
- Archivado continuo de WAL (asíncrono, con spool), completo semanal e
  incremental diario (crond dentro del contenedor del primario).
- Cifrado AES-256-CBC (el que ofrece pgBackRest) con passphrase en Docker
  secret; compresión zstd.
- Repo 1 local (volumen) + Repo 2 S3 compatible fuera del servidor, con su
  propia passphrase.
- Réplica de streaming por TLS verificado con slot de replicación limitado
  (`max_slot_wal_keep_size`) para no llenar el disco del primario.
- `infra/backup/restore-test.sh` restaura en un Postgres desechable **sin
  archivado** (no contamina el repositorio) y verifica la integridad con
  `sb-admin verify`.

## Consecuencias
- Recuperación a cualquier instante (PITR).
- El crond y Postgres comparten contenedor: simple y con acceso local al
  PGDATA; si falla un respaldo queda en `/var/log/pgbackrest` (agregar alerta
  sobre la antigüedad del último respaldo en una siguiente iteración).
