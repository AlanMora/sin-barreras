# 0003. Valkey en lugar de Redis

- **Estado:** Aceptado
- **Fecha:** 2026-10-09

## Contexto
La arquitectura usa un almacén en memoria para sesiones, rate limit, pub/sub
entre réplicas del BFF y los Streams del registro. Redis cambió de licencia a
partir de la 7.4 (RSALv2/SSPL y después AGPLv3 en Redis 8). Para un sujeto
obligado del gobierno municipal, una licencia restrictiva o copyleft de red es
un riesgo legal y de adquisiciones.

## Decisión
Usar **Valkey 9.1** (fork de Redis bajo licencia BSD-3, mantenido por la Linux
Foundation). La API y el protocolo son compatibles; el cliente Rust `fred` lo
soporta oficialmente. En el código y la documentación nos referimos a él como
"Valkey"; la palabra "Redis" en el `CLAUDE.md` se entiende como Valkey.

Configuración obligatoria (`infra/valkey/valkey.conf`):
- `maxmemory-policy noeviction`: **nunca** se expulsan llaves por memoria. Un
  evento expulsado sería un registro perdido. Si la memoria se llena, las
  escrituras fallan de forma visible y se alerta.
- AOF activado con `appendfsync everysec` + instantáneas RDB.
- Solo TLS (`port 0`), ACL por servicio con mínimo privilegio, usuario
  `default` deshabilitado y contraseñas guardadas como hash SHA-256.

## Ventana de pérdida y su mitigación
Con `everysec`, ante la caída del nodo pueden perderse hasta ~1 s de escrituras
ya confirmadas. Para los eventos del registro el BFF usará `WAITAOF 1 0` (Fase
2), **fuera de la ruta crítica** del ciudadano: el mensaje se reenvía al C5 sin
esperar, y el evento solo se considera publicado cuando Valkey confirma el
fsync. Mientras tanto se conserva en un búfer acotado del BFF. Se descartó
`appendfsync always` porque penaliza la latencia de todas las operaciones
(sesiones, rate limit) y no solo las del registro.

## Requisito del host
Valkey necesita `vm.overcommit_memory = 1` en el kernel del nodo; sin él, la
reescritura del AOF o el guardado RDB (que hacen `fork`) pueden fallar con poca
memoria. No se puede ajustar desde el contenedor. Es requisito de los nodos de
Swarm (Fase 8). En desarrollo solo produce una advertencia.

## Consecuencias
- Sin riesgo de licencia.
- Comportamiento idéntico a Redis para nuestro uso.
- Valkey es un punto único de falla hasta que se defina su alta disponibilidad
  (Sentinel o réplica) en la Fase 8.
