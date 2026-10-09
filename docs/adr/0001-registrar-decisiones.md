# 0001. Registrar decisiones con ADRs

- **Estado:** Aceptado
- **Fecha:** 2026-10-09

## Contexto
Sin Barreras es un sistema de emergencias con requisitos legales (registro con
valor probatorio, LFPDPPP). Ante una auditoría o investigación hay que poder
explicar por qué el sistema funciona como funciona.

## Decisión
Toda decisión técnica relevante se registra como ADR en `docs/adr/`, numerada,
en español, con contexto, decisión, alternativas y consecuencias. Los ADR
aceptados son inmutables; un cambio de decisión se registra en un ADR nuevo.

## Consecuencias
- La historia de las decisiones viaja con el código y se revisa en los PR.
- Hay un costo pequeño de escritura por cada decisión.
