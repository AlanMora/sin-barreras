# 0015. Valkey con `appendfsync always` (reemplaza la parte de durabilidad del ADR-0003)

- **Estado:** Aceptado. Reemplaza la sección "Ventana de pérdida y su mitigación" del ADR-0003.
- **Fecha:** 2026-10-09

## Contexto
El ADR-0003 proponía `appendfsync everysec` y usar `WAITAOF` solo para los
eventos del registro. Al implementar el BFF se vio que **el `ack` al ciudadano
también depende de la durabilidad de Valkey**: con `everysec`, si el nodo cae
en el segundo posterior al `ack`, un "necesito ayuda" ya confirmado podría
perderse, y el ciudadano no lo reenviaría porque ya lo vio como enviado.

## Decisión
`appendfsync always`: Valkey hace fsync del AOF antes de responder a cada
escritura. Cada operación confirmada ya está en disco.

## Consecuencias
- Cero ventana de pérdida para mensajes confirmados y eventos del registro.
- Latencia por escritura de ~0.1–1 ms en SSD local. El volumen del programa
  (decenas de operaciones por segundo) está órdenes de magnitud por debajo del
  límite.
- **Requisito:** el volumen de Valkey debe estar en SSD local, no en disco de
  red lento.
- Se elimina la complejidad de `WAITAOF`.
