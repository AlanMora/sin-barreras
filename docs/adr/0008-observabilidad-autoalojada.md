# 0008. Observabilidad autoalojada y trazas de la PWA sin SDK

- **Estado:** Aceptado
- **Fecha:** 2026-10-09

## Contexto
El proyecto pide trazas OpenTelemetry de punta a punta, métricas y alertas. Los
datos son del propio programa: no se envían a terceros. La PWA tiene un
presupuesto de 60 KB, y el SDK web de OpenTelemetry pesa por sí solo entre 30 y
50 KB comprimido.

## Decisión
- Stack autoalojado: **OpenTelemetry Collector → Prometheus (métricas),
  Tempo (trazas), Grafana (tableros) y Alertmanager (alertas)**. Nada sale de
  nuestra infraestructura.
- La PWA **no** incluye el SDK de OpenTelemetry. Genera un `traceparent` W3C
  por operación (unos cuantos bytes de código) y lo envía en el handshake y en
  cada mensaje; la traza completa empieza en el BFF con ese identificador.
- Los logs se emiten en JSON por stdout y nunca contienen mensajes, ubicación
  ni IP (`service-kit::telemetry`, `infra/nginx/pwa.conf`).

## Consecuencias
- Trazas correlacionadas PWA → BFF → C5 sin afectar el presupuesto.
- No hay métricas de rendimiento del navegador (Web Vitals) con el SDK; si se
  necesitan, se enviarán como eventos propios del registro.
