# Plan por fases

| Fase | Entregables | Estado |
|---|---|---|
| 0. Fundaciones | Monorepo, toolchains fijadas, `service-kit`, esqueletos de servicios con health, PWA con presupuesto, compose con TLS + ACL + AOF, Dockerfiles mínimos, CI, ADRs 0001–0009 | ✅ Hecha |
| 1. Contrato + BFF + c5-mock | `contrato-eventos.md`, crate `protocol` + tipos TS generados, WebSocket con versión, token anónimo, `seq` + reenvío, idempotencia por `client_msg_id`, ACKs, pub/sub entre réplicas, `C5Gateway` + adaptadores, simulador de operador | ✅ Hecha |
| 2. Registro completo | Eventos a Valkey Streams, record-writer con consumer groups y no pérdida, cifrado envelope AES-256-GCM, cadena de hash por sesión + checkpoints, roles y append-only en Postgres, `verify-chain`, `modelo-datos.md` | ✅ Hecha |
| 3. PWA base | Sistema de diseño, componentes WAI-ARIA, i18n es/en, QR, aviso de privacidad, canales, chat | Siguiente |
| 4. Confiabilidad | Backoff + jitter, cola IndexedDB, recuperación por `seq`, iOS, respaldo HTTP, ubicación | Pendiente |
| 5. Silencioso, estados, video | Modo silencioso + salida rápida, estados del incidente, `VideoCallProvider` | Pendiente |
| 6. Auditoría a11y + ajustes | WCAG 2.1 AA, ajustes, axe en E2E, checklist de lectores de pantalla | Pendiente |
| 7. Seguridad | Rotación de llaves de QR, rate limit, CSP, CLI de investigación + bitácora, pgAudit | Pendiente |
| 8. Swarm + observabilidad | Stack de Swarm, réplica, pgBackRest, OTel/Prometheus/Tempo/Grafana/Alertmanager, E2E | Pendiente |
