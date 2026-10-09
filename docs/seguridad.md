# Seguridad

Resumen de los controles. El detalle de cada decisión está en los ADRs.

## Amenazas consideradas y controles

| Amenaza | Control |
|---|---|
| Interceptar tráfico | TLS en todo: Traefik (público), y TLS interno a Valkey (solo TLS) y Postgres (`hostssl`, TLS 1.3, `verify-full`) — ADR-0005 |
| Robar el registro (disco, respaldo, base) | Contenido, ubicación e IP cifrados con AES-256-GCM por sesión; la DEK envuelta con HPKE para una llave pública cuya privada está fuera de línea — ADR-0014 |
| Comprometer un servicio en línea | El BFF solo tiene la llave **pública**; `record-writer` no tiene llaves; roles de BD con mínimo privilegio por columna; ACL de Valkey por servicio — ADR-0014, 0003 |
| Alterar o borrar registros | Solo inserción por permisos + triggers; cadena de hash por sesión + checkpoints globales; `sb-admin verify` — ADR-0016 |
| Leer registros sin dejar rastro | Funciones de investigación con bitácora obligatoria; sin SELECT directo — ADR-0021 |
| Suplantar una sesión | Token de 256 bits, solo su hash en Valkey, rotado en cada reconexión — ADR-0010 |
| QR falsificados | HMAC-SHA256 truncado a 128 bits, llaves con id rotables, lista de puntos revocados (`QR_REVOKED_POINTS`); un QR inválido NO bloquea la ayuda — ADR-0007 |
| Inundación / abuso | Rate limiting por sesión que pospone sin perder; observación por IP; límite duro activable — ADR-0020 |
| Ataques entre sitios (XSS, CSWSH, CSRF, clickjacking) | CSP estricta sin `unsafe-inline`; validación de `Origin` en WebSocket y API; `frame-ancestors 'none'`; sin cookies |
| Entradas maliciosas | Esquema estricto (`deny_unknown_fields`), límites de tamaño (16 KB por frame, 256 KB por petición, 2000 caracteres por mensaje), validación de rangos; SQL siempre parametrizado |
| Fugas por logs | Los logs nunca contienen mensajes, ubicación ni IP (BFF, Nginx y Traefik configurados sin IP) |
| Cadena de suministro | Versiones exactas, lockfiles congelados, edad mínima de publicación (pnpm), acciones de CI fijadas por SHA, Dependabot, imágenes distroless no-root |

## Cabeceras (PWA y BFF)

`Content-Security-Policy` (solo mismo origen), `X-Content-Type-Options: nosniff`,
`Referrer-Policy: no-referrer`, `X-Frame-Options: DENY`,
`Cross-Origin-Opener-Policy`, `Cross-Origin-Resource-Policy`, `Permissions-Policy`
(geolocalización, cámara y micrófono solo del propio origen),
`Cache-Control: no-store` en el API. HSTS lo agrega Traefik en producción.

## Pendiente de definición (no técnico)

- Quién puede tener el rol `sb_investigator` y la llave privada; custodia dual.
- Formato de folio para las investigaciones.
- Periodo de retención y procedimiento ARCO (ADR-0017).
- Texto definitivo del aviso de privacidad (borrador en la PWA).
