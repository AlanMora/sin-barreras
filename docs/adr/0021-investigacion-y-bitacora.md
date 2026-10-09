# 0021. Investigación con bitácora de accesos obligatoria, aplicada por la base de datos

- **Estado:** Aceptado
- **Fecha:** 2026-10-09

## Contexto
Toda consulta a registros sensibles debe quedar registrada: quién, cuándo y
qué. Si la bitácora dependiera solo del código de una herramienta, cualquiera
con credenciales de base de datos podría leer sin dejar rastro.

## Decisión
- El rol `sb_investigator` **no tiene SELECT** sobre ninguna tabla. Solo puede
  ejecutar funciones `SECURITY DEFINER` del esquema `investigation`
  (`find_sessions`, `read_session`, `session_keys`), que **primero** insertan en
  `registry.access_log` (quién lo pide, motivo o folio obligatorio, acción, sesión,
  filas devueltas, usuario de BD, IP) y **después** devuelven los datos.
- `access_log` es de solo inserción (permisos + triggers).
- Además, `log_statement = 'all'` para ese rol: cada sentencia queda en el log
  del servidor (defensa en profundidad; pgAudit es opcional si se construye una
  imagen de Postgres con la extensión).
- Las funciones corren en el **primario** (la réplica es de solo lectura y no
  puede escribir la bitácora). Las investigaciones son esporádicas: la carga es
  despreciable.
- `sb-admin investigate` verifica la **integridad** de la cadena de la sesión con
  las filas recibidas antes de mostrarlas, y descifra con la llave **privada**
  de investigación, que solo existe fuera de los servidores en línea.

## Consecuencias
- No hay forma de leer contenido sensible sin dejar rastro en la base.
- El contenido sigue cifrado en tránsito desde la base hasta el equipo del
  investigador; solo se descifra ahí.
- El área jurídica debe definir quién puede tener el rol y la llave, y el
  formato de folio (ver `docs/seguridad.md`).
