# 0010. Sesiones anónimas con token rotativo y vida deslizante

- **Estado:** Aceptado
- **Fecha:** 2026-10-09

## Contexto
El ciudadano no tiene cuenta. Necesitamos identificar su sesión para reanudar
la conversación tras una desconexión (iOS suspende el WebSocket al bloquear la
pantalla) sin exponer un identificador que alguien más pueda usar.

Un token de vida fija y corta (por ejemplo, 15 min) es peligroso en una
emergencia: una persona escondida, con el teléfono bloqueado durante 40 minutos,
perdería su conversación con el operador al volver.

## Decisión
- El BFF genera un token de 256 bits aleatorios (base64url) en cada `welcome`.
- En Valkey solo se guarda su SHA-256. Quien lea Valkey no puede suplantar.
- **Rotación:** cada reanudación emite un token nuevo e invalida el anterior.
  Un token robado deja de servir en cuanto el dueño legítimo reconecta.
- **Vida deslizante:** el token vale mientras la sesión exista. La sesión expira
  tras inactividad: 30 min sin incidente (`SESSION_IDLE_TTL_SECS`) y 12 h con
  un incidente abierto (`SESSION_INCIDENT_TTL_SECS`). Al cerrarse el incidente,
  vuelve a 30 min.
- Una sesión expirada o un token inválido **no bloquean**: se crea una sesión
  nueva y la PWA reenvía sus pendientes.

## Consecuencias
- La conversación sobrevive a bloqueos largos del teléfono.
- Robar el token exige acceso al almacenamiento del teléfono; su utilidad dura
  hasta la siguiente reconexión del dueño.
- La comparación del hash se hace dentro de Valkey (Lua). No es de tiempo
  constante, pero compara hashes SHA-256 de valores aleatorios de 256 bits, lo
  que hace inviable un ataque de tiempo.
