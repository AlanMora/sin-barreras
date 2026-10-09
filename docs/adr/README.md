# Registro de decisiones de arquitectura (ADR)

Cada decisión técnica importante se registra aquí con su contexto, la decisión
y sus consecuencias. Un ADR aceptado no se edita: si la decisión cambia, se
escribe uno nuevo que lo reemplaza y se marca el anterior como "Reemplazado".

| # | Decisión | Estado |
|---|---|---|
| [0001](0001-registrar-decisiones.md) | Registrar decisiones con ADRs | Aceptado |
| [0002](0002-monorepo-y-workspace.md) | Monorepo con cargo workspace en la raíz y `/crates` compartidos | Aceptado |
| [0003](0003-valkey-en-lugar-de-redis.md) | Valkey en lugar de Redis | Aceptado |
| [0004](0004-failover-manual-de-postgres.md) | Failover manual de Postgres | Aceptado |
| [0005](0005-tls-interno-en-todos-los-entornos.md) | TLS interno en todos los entornos, incluido desarrollo | Aceptado |
| [0006](0006-stack-de-la-pwa.md) | Stack y herramientas de la PWA | Aceptado |
| [0007](0007-ninguna-validacion-bloquea-la-ayuda.md) | Ninguna validación bloquea la petición de ayuda | Aceptado |
| [0008](0008-observabilidad-autoalojada.md) | Observabilidad autoalojada y trazas de la PWA sin SDK | Aceptado |
| [0009](0009-imagenes-minimas.md) | Imágenes mínimas con healthcheck integrado en el binario | Aceptado |

Plantilla: copia [`0000-plantilla.md`](0000-plantilla.md).
