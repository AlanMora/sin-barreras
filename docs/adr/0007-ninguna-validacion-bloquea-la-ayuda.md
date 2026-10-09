# 0007. Ninguna validación bloquea la petición de ayuda

- **Estado:** Aceptado
- **Fecha:** 2026-10-09

## Contexto
El sistema valida la firma del QR, aplica rate limit por IP y por sesión y
valida estrictamente las entradas. Cada una de esas defensas podría, por sí
sola, dejar sin ayuda a una persona real: un QR maltratado o reimpreso mal, un
operador móvil con CGNAT (miles de personas detrás de una IP), un navegador
viejo que envía un campo mal formado.

## Decisión
Principio de diseño: **las defensas degradan, no rechazan, la petición de
ayuda.**
- Firma de QR inválida o ausente: la sesión continúa marcada como "punto de
  entrada no verificado"; el operador lo ve y la ubicación del QR no se usa
  como respaldo.
- Rate limit: protege recursos (por ejemplo, limita mensajes por segundo), pero
  la creación del incidente y el primer mensaje de ayuda de una sesión nunca
  se rechazan; se marcan para revisión. Umbrales por IP altos (CGNAT) y control
  fino por sesión. Los umbrales se documentan en la Fase 7.
- Validación de entrada: se rechaza lo malicioso o imposible de interpretar,
  pero un campo opcional inválido se descarta con un evento de registro en
  lugar de rechazar el mensaje completo.
- Todo rechazo o degradación queda registrado como evento.

## Consecuencias
- Un atacante puede crear sesiones "no verificadas"; el C5 debe poder
  distinguirlas. Pregunta abierta en `docs/c5-preguntas.md`.
- La protección contra abuso masivo se apoya más en límites de recursos y
  en la observación que en el rechazo.
