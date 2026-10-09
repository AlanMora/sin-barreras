# 0014. Cifrado del registro: sobre por sesión con llave pública de investigación

- **Estado:** Aceptado
- **Fecha:** 2026-10-09

## Contexto
El registro guarda conversaciones de emergencia de personas con discapacidad,
ubicación exacta e IP. El proyecto exige cifrarlos a nivel de aplicación con
AES-256-GCM, llaves fuera del código y de la base, y un diseño de rotación.

## Decisión
**Cifrado de sobre (envelope) con llave asimétrica de investigación:**

1. Cada sesión tiene su propia **DEK** (256 bits). El contenido sensible de cada
   evento se cifra con **AES-256-GCM**, nonce aleatorio de 96 bits y como dato
   asociado (AAD) `event_id | session_id | kind`: un texto cifrado movido a
   otro registro no se puede descifrar.
2. La DEK se envuelve con **HPKE (RFC 9180)**: DHKEM(X25519, HKDF-SHA256) +
   HKDF-SHA256 + AES-256-GCM, hacia la **llave PÚBLICA de investigación**, y se
   guarda en `registry.session_keys` (evento `session_key_created`).
3. Para que las réplicas del BFF compartan la DEK de una sesión activa, se
   guarda en Valkey envuelta con una **llave operativa** simétrica.
4. El **BFF solo tiene la llave pública**: puede cifrar, no puede descifrar el
   historial. `record-writer` no tiene ninguna llave: guarda texto cifrado.
5. La **llave privada** solo la usa `sb-admin` para investigaciones, en un
   equipo fuera de los servidores en línea.
6. Si un evento sensible no tiene sesión o su DEK no está disponible, se cifra
   con una DEK propia envuelta dentro del mismo registro (`key_ref: inline`).
   Nunca se guarda contenido sensible en claro: `record-writer` rechaza (a la
   cola de errores) cualquier evento que lo traiga.

## Rotación
| Llave | Cómo se rota | Efecto |
|---|---|---|
| Pública/privada de investigación | Nuevo par con nuevo id; el BFF usa la nueva para sesiones nuevas | Las sesiones viejas siguen con la llave anterior (cada envoltura guarda su `recipient_key_id`). Opcional: re-envolver las DEK viejas con la nueva (se insertan filas nuevas en `session_keys`) y destruir la privada vieja |
| Operativa | Se agrega la nueva al inicio de `RECORD_OPERATIONAL_KEYS` y se conserva la anterior hasta que expiren las sesiones activas (≤ 12 h) | Sin interrupción |
| DEK | Una por sesión | Rotación natural |
| Algoritmo | Cada envoltura y cada texto cifrado guardan su `alg` | Se puede migrar, por ejemplo, a un KEM post-cuántico (X-Wing) sin romper lo anterior |

Procedimiento detallado: `docs/llaves.md`.

## Alternativas consideradas
- Llave maestra simétrica en el BFF: un BFF comprometido descifraría todo el
  historial.
- Una DEK por evento sin sesión: impide el borrado criptográfico por sesión.

## Consecuencias
- Comprometer un servicio en línea no expone el historial cifrado.
- Perder la llave privada de investigación significa perder el acceso al
  contenido. Debe custodiarse con respaldo (ver `docs/llaves.md`).
