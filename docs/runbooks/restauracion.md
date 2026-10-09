# Runbook: restaurar respaldos (y prueba periódica)

Respaldos: pgBackRest, completo semanal + incremental diario + WAL continuo,
cifrados (AES-256), en el volumen local y en una copia fuera del servidor (S3).

## Prueba de restauración (mensual, obligatoria)
```bash
infra/backup/restore-test.sh
```
Hace un respaldo completo, lo restaura en un Postgres desechable (sin
archivado: no toca el repositorio del primario) y corre `sb-admin verify`
sobre lo restaurado. Debe terminar con:

```
✓ PRUEBA DE RESTAURACIÓN SUPERADA: respaldo cifrado restaurable, íntegro y completo.
```
Resultado de la primera ejecución (desarrollo, 2026-10-09): 4,739 registros,
223 cadenas, 18 checkpoints verificados; 34.5 MB → 4.7 MB cifrado.

## Restauración real
1. Detener `record-writer` (`docker service scale sin-barreras_record-writer=0`):
   los eventos esperan en Valkey.
2. Restaurar en un volumen nuevo (mismos pasos que `restore-test.sh`, eligiendo
   `--type=time --target="AAAA-MM-DD HH:MM:SS"` para recuperar a un instante).
3. Verificar con `sb-admin verify`.
4. Apuntar el servicio `postgres` al volumen restaurado y escalar `record-writer`
   de nuevo: los eventos pendientes se guardan sin duplicarse (idempotencia).
