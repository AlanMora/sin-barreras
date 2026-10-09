# 0006. Stack y herramientas de la PWA

- **Estado:** Aceptado
- **Fecha:** 2026-10-09

## Contexto
La PWA debe cargar en menos de 2 s en 3G lenta, con un bundle inicial menor a
60 KB comprimido, ser totalmente accesible y no depender de librerías de
componentes de React.

## Decisión
- **Preact 11** + `@preact/signals` + `preact-iso`, sin `preact/compat`.
- **TypeScript 7** en modo estricto (`noUncheckedIndexedAccess`,
  `exactOptionalPropertyTypes`).
- **Vite 8** con `@preact/preset-vite` y `vite-plugin-pwa` (registro
  `prompt`: una actualización nunca recarga la página a media emergencia).
- **Tailwind CSS 4** con tokens propios en `@theme` (`src/styles.css`).
- **Biome** como linter y formateador único (sustituye ESLint + Prettier con
  una sola dependencia), con las reglas de accesibilidad en nivel `error`.
- **Vitest** para pruebas unitarias.
- **Presupuesto de rendimiento** verificado por `scripts/check-budget.mjs`
  sin dependencias extra: suma HTML + entrada + imports estáticos + CSS
  comprimidos con gzip nivel 9 y hace fallar el build si superan 60 KB. Se
  mide con gzip porque es lo que sirve `nginx:alpine` (no trae brotli).
- **Versiones exactas** (`saveExact`) y lockfile congelado en CI.
- Servido por `nginxinc/nginx-unprivileged` (la imagen oficial de Nginx Inc.
  que corre como no-root), en lugar de `nginx:alpine` que corre como root.

## Paleta (propuesta inicial, se afina en la Fase 3)
Azul marino institucional (`#0b2545`) y naranja de acento (`#f47b20`).
Hallazgo de accesibilidad: el naranja con texto blanco da 2.73:1 y **no
cumple** AA. Regla: los botones naranjas llevan texto azul marino (5.64:1) y el
naranja como texto usa `#b54708` (5.43:1).

## Consecuencias
- Bundle inicial actual: 10.7 KB gzip (18 % del presupuesto).
- Preact 11 y TS 7 son versiones mayores recientes; se verificó la
  compatibilidad de peers de todas las dependencias.
