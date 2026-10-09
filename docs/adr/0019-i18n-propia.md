# 0019. i18n propia, tipada y sin dependencias

- **Estado:** Aceptado
- **Fecha:** 2026-10-09

## Contexto
El `CLAUDE.md` pide "i18n con una librería ligera", español por defecto, inglés
y estructura lista para más idiomas, dentro de un presupuesto de 60 KB.

## Decisión
Un módulo propio de ~40 líneas (`src/i18n/`):
- Diccionarios planos por idioma; el español es la fuente de verdad.
- Cada idioma nuevo tiene el tipo `Record<MessageKey, string>`: **el compilador
  exige todas las claves**. Una prueba lo verifica además en tiempo de ejecución.
- Interpolación `{nombre}` y el idioma activo como signal (todo se re-renderiza).
- `lang` del documento sincronizado para los lectores de pantalla.

## Alternativas consideradas
- `rosetta`, `preact-i18n`, `lingui`: más peso o menos tipado, y ninguna aporta
  algo que necesitemos (no hay plurales complejos ni ICU en estos textos).

## Consecuencias
- 0 KB de dependencias. Agregar un idioma = un archivo.
