# Registro de decisiones de arquitectura (ADR)

Cada decisión técnica importante se registra aquí con su contexto, la decisión
y sus consecuencias. Un ADR aceptado no se edita: si la decisión cambia, se
escribe uno nuevo que lo reemplaza y se marca el anterior como "Reemplazado".

| # | Decisión | Estado |
|---|---|---|
| [0001](0001-registrar-decisiones.md) | Registrar decisiones con ADRs | Aceptado |
| [0002](0002-monorepo-y-workspace.md) | Monorepo con cargo workspace en la raíz y `/crates` compartidos | Aceptado |
| [0003](0003-valkey-en-lugar-de-redis.md) | Valkey en lugar de Redis | Aceptado (durabilidad: ver 0015) |
| [0004](0004-failover-manual-de-postgres.md) | Failover manual de Postgres | Aceptado |
| [0005](0005-tls-interno-en-todos-los-entornos.md) | TLS interno en todos los entornos, incluido desarrollo | Aceptado |
| [0006](0006-stack-de-la-pwa.md) | Stack y herramientas de la PWA | Aceptado |
| [0007](0007-ninguna-validacion-bloquea-la-ayuda.md) | Ninguna validación bloquea la petición de ayuda | Aceptado |
| [0008](0008-observabilidad-autoalojada.md) | Observabilidad autoalojada y trazas de la PWA sin SDK | Aceptado |
| [0009](0009-imagenes-minimas.md) | Imágenes mínimas con healthcheck integrado en el binario | Aceptado |
| [0010](0010-sesiones-anonimas.md) | Sesiones anónimas con token rotativo y vida deslizante | Aceptado |
| [0011](0011-entrega-garantizada-al-c5.md) | Entrega garantizada y ordenada de mensajes al C5 | Aceptado |
| [0012](0012-cliente-valkey-y-tls.md) | Cliente `fred` y `aws-lc-rs` como proveedor criptográfico | Aceptado |
| [0013](0013-traefik-tambien-en-desarrollo.md) | Traefik también en desarrollo y BFF con 2 réplicas | Aceptado |
| [0014](0014-cifrado-del-registro.md) | Cifrado de sobre por sesión con llave pública de investigación (HPKE) | Aceptado |
| [0015](0015-valkey-fsync-always.md) | Valkey con `appendfsync always` (reemplaza parte del 0003) | Aceptado |
| [0016](0016-registro-sin-perdida.md) | Registro sin pérdida: escritura atómica, consumer groups e idempotencia | Aceptado |
| [0017](0017-retencion.md) | Retención configurable, sin borrado automático (borrado criptográfico) | Aceptado |
| [0018](0018-transporte-http-de-respaldo.md) | Transporte HTTP de respaldo con la misma semántica que el WebSocket | Aceptado |
| [0019](0019-i18n-propia.md) | i18n propia, tipada y sin dependencias | Aceptado |
| [0020](0020-rate-limiting.md) | Rate limiting que degrada y nunca bloquea el pedido de ayuda | Aceptado |
| [0021](0021-investigacion-y-bitacora.md) | Investigación con bitácora de accesos obligatoria en la BD | Aceptado |
| [0022](0022-respaldos-pgbackrest.md) | Respaldos con pgBackRest, réplica y prueba de restauración | Aceptado |

Plantilla: copia [`0000-plantilla.md`](0000-plantilla.md).
