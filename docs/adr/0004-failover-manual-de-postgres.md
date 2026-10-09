# 0004. Failover manual de Postgres

- **Estado:** Aceptado
- **Fecha:** 2026-10-09

## Contexto
Postgres corre como primario + réplica de streaming en nodos distintos. Si el
primario cae, alguien o algo debe promover la réplica a primario ("failover").
Las opciones son la promoción manual con un procedimiento documentado o la
automática con Patroni (que además requiere etcd o Consul).

Dato clave de nuestra arquitectura: **una caída de Postgres no afecta al
ciudadano**. El BFF solo publica en Valkey Streams; los eventos esperan en el
stream hasta que la base de datos vuelve (regla de oro 1), y record-writer
confirma (XACK) solo después de escribir (regla de oro 2).

## Decisión
Failover **manual** con un runbook paso a paso y alertas:
- Alerta cuando record-writer no puede escribir y cuando crece el retraso del
  stream.
- El runbook (Fase 8) cubre: verificar que el primario realmente cayó, aislarlo
  (para evitar dos primarios), promover la réplica (`pg_promote()`), reapuntar
  record-writer y reconstruir la nueva réplica.

## Alternativas consideradas
- **Patroni + etcd**: failover en segundos, pero agrega un clúster de consenso
  (mínimo 3 nodos de etcd) y más piezas que operar, y un failover automático
  mal configurado puede provocar dos primarios (*split-brain*), lo que en un
  registro con cadena de integridad es peor que unos minutos de espera.

## Consecuencias
- Tiempo de recuperación de minutos (los que tarde el operador), sin impacto
  en la atención de emergencias ni pérdida de registros.
- El stream de Valkey debe tener memoria suficiente para acumular eventos
  durante una caída prolongada; se dimensiona y alerta en la Fase 8.
- Se puede migrar a Patroni más adelante con un ADR nuevo.
