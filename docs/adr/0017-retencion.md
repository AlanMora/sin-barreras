# 0017. Retención configurable, sin borrado automático

- **Estado:** Aceptado (el periodo está pendiente de definición por el área jurídica)
- **Fecha:** 2026-10-09

## Contexto
El periodo de conservación lo define el área jurídica del municipio
(LFPDPPP, ley de archivos, normativa de sujetos obligados). Mientras tanto no
puede borrarse nada. Por otro lado, el registro es append-only y encadenado:
borrar filas rompería la verificación de integridad.

## Decisión
- **No hay borrado automático.** Ningún proceso borra registros.
- El periodo será un parámetro (`RETENTION_DAYS`). Sin definir = conservar
  indefinidamente. Hoy no está definido.
- Cuando se defina, la eliminación se hará por **borrado criptográfico**: se
  destruye la DEK envuelta de las sesiones vencidas (`registry.session_keys`)
  y su contenido queda ilegible para siempre, mientras la cadena de integridad
  sigue verificando, porque el hash cubre el texto cifrado y no el claro.
  Los metadatos no sensibles (tipos, tiempos, canal) se conservan para
  estadística.
- El mismo mecanismo atiende una **solicitud ARCO de cancelación** que proceda.
- La operación requerirá: una migración que cree una función `SECURITY
  DEFINER` exclusiva, un rol dedicado sin uso cotidiano, autorización
  documentada y registro en la bitácora de accesos (Fase 7).

## Consecuencias
- Cumplimiento sin sacrificar la integridad del registro.
- Pendiente: definición jurídica del periodo y del procedimiento ARCO.
