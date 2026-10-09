# Runbook: el Postgres primario cayó (failover manual, ADR-0004)

**Impacto en el ciudadano: ninguno.** El BFF sigue atendiendo; los eventos del
registro esperan en Valkey (`sb:rec:events`) y nada se pierde.

Alertas típicas: `RegistroConRetraso`, `StreamDelRegistroCreciendo`.

## 1. Confirmar
```bash
docker service ps sin-barreras_postgres          # ¿la tarea está caída o reiniciando?
docker service logs --tail 100 sin-barreras_postgres
```
Si Swarm lo reinicia solo y vuelve en minutos, **no hacer failover**: los
eventos pendientes se guardarán solos. Vigilar `sb_record_writer_delay_seconds`.

## 2. Si el nodo primario no volverá pronto
1. **Aislar** el primario viejo para que nunca vuelva como primario (evita dos
   primarios): `docker node update --availability drain nodo-a`.
2. **Promover** la réplica:
   ```bash
   docker exec $(docker ps -qf name=sin-barreras_postgres-replica) \
     psql -U postgres -c "SELECT pg_promote(wait => true)"
   ```
3. **Reapuntar**: el servicio `postgres` (nombre que usan record-writer y migrate)
   debe resolver a la réplica promovida. Cambiar las etiquetas y redesplegar:
   `docker node update --label-rm sb.postgres --label-add sb.postgres=primary nodo-b`
   y actualizar el stack con el volumen de la réplica como datos del primario
   (ver sección "Reconstrucción").
4. Verificar: `sb_record_writer_delay_seconds` baja a 0; `sb-admin verify` OK.

## 3. Reconstrucción de la nueva réplica
Con el nodo viejo reparado, vaciar su volumen y dejar que el servicio
`postgres-replica` se inicialice solo con `pg_basebackup` desde el nuevo primario.

## 4. Después
- `pgbackrest --stanza=sin-barreras stanza-upgrade` si cambió el sistema.
- Respaldo completo inmediato.
- Registrar el incidente.
