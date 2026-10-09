# 0013. Traefik también en desarrollo y BFF con 2 réplicas desde local

- **Estado:** Aceptado
- **Fecha:** 2026-10-09

## Contexto
En producción Traefik enruta `/` a Nginx (PWA) y `/ws` al BFF, balanceando
entre réplicas. Si en desarrollo se habla directo con una sola réplica, los
errores de enrutamiento entre réplicas aparecen por primera vez en producción.

## Decisión
- `docker compose --profile apps` levanta Traefik con el mismo ruteo y **2
  réplicas del BFF**. La PWA y el WebSocket comparten origen
  (`http://localhost:8000`).
- Traefik verifica `/ready` de cada réplica y deja de enviarle tráfico si falla.
- El log de accesos de Traefik descarta la IP del cliente
  (`ClientAddr`, `ClientHost`, `ClientPort`) y todas las cabeceras.

## Consecuencias
- El smoke test (`node scripts/smoke-ws.mjs`) recorre exactamente el camino de
  producción, incluida la reanudación en una réplica distinta.
- Traefik necesita leer el socket de Docker en desarrollo (solo lectura). En
  Swarm se usará el proveedor de Swarm (Fase 8).
