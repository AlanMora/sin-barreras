# 0002. Monorepo con cargo workspace en la raíz y `/crates` compartidos

- **Estado:** Aceptado
- **Fecha:** 2026-10-09

## Contexto
Hay tres servicios Rust (BFF, record-writer, c5-mock) y una PWA. El BFF y
record-writer comparten el formato de eventos del registro; el BFF y la PWA
comparten el contrato del WebSocket. Si cada servicio define sus propios tipos,
tarde o temprano divergen, y una divergencia en el registro es un registro
ilegible.

## Decisión
- Un solo repositorio.
- `Cargo.toml` del workspace en la **raíz**: un único `Cargo.lock`, una única
  caché de compilación en CI y versiones de dependencias centralizadas en
  `[workspace.dependencies]`.
- Servicios desplegables en `/apps/*`; código compartido (no desplegable) en
  `/crates/*`. Esta carpeta no estaba en la estructura original; se agrega
  porque el código compartido no es una "app".
- Crates compartidos previstos: `service-kit` (logs, config, health, apagado;
  ya existe), `protocol` (contrato PWA↔BFF, Fase 1), `events` (sobre de
  eventos del registro, Fase 2), `crypto` (Fase 2), `integrity` (Fase 2).
- La PWA usa un workspace de pnpm; los tipos TypeScript del contrato se
  generarán desde el crate `protocol` (Fase 1).
- Toolchains fijadas: `rust-toolchain.toml` (Rust 1.99.0) y `.nvmrc` (Node 24).
- `c5-mock` se escribe en Rust para reutilizar los tipos y el `service-kit`.

## Alternativas consideradas
- Workspace en `/apps`: obliga a rutas `../crates` incómodas y no aporta nada.
- Repos separados por servicio: multiplica la coordinación de versiones del
  contrato sin beneficio para un equipo pequeño.

## Consecuencias
- Un cambio de contrato rompe la compilación de ambos lados en el mismo PR.
- Lints estrictos comunes (`unsafe_code = forbid`, `unwrap_used = deny`) para
  todo el código Rust.
