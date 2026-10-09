# Preguntas para el equipo del C5

Lista viva de dudas que necesitamos resolver para escribir el adaptador real
de `C5Gateway`. Mientras no haya respuesta, el BFF usa el simulador
`apps/c5-mock`, cuya API es **inventada** y no representa la del C5.

## Integración técnica
1. ¿Qué protocolo exponen para recibir incidentes y mensajes (REST, WebSocket,
   gRPC, colas)? ¿Hay especificación (OpenAPI/AsyncAPI)?
2. ¿Cómo nos autenticamos (mTLS, OAuth2 client credentials, API key)? ¿Cómo se
   rotan las credenciales?
3. ¿Qué identificador devuelve el C5 al crear un incidente? ¿Es único y estable
   para siempre? (Lo guardamos junto al nuestro para cruzar registros.)
4. ¿Los mensajes del operador llegan por push (socket/webhook) o hay que
   consultarlos? ¿Traen identificador y marca de tiempo propios?
5. ¿El C5 confirma la entrega y la lectura de los mensajes del ciudadano? (Lo
   necesitamos para los indicadores "entregado" y "leído".)
6. ¿Qué estados de incidente maneja el C5 y cuál es su catálogo exacto
   (atendiendo, unidad en camino, cerrado…)? ¿Incluyen tiempo estimado?
7. ¿Garantizan idempotencia si reenviamos una petición por un reintento? ¿Qué
   llave usamos?
8. ¿Cómo aceptan las actualizaciones de ubicación (formato, frecuencia máxima,
   precisión)?
9. ¿Qué pasa del lado del C5 si nuestra conexión se cae a media conversación?
   ¿Cómo se reanuda?
10. ¿Tienen ambiente de pruebas (sandbox) con datos ficticios?
11. ¿Cuáles son sus límites de tasa y tiempos de respuesta esperados?

## Lo que asume hoy el adaptador (ADR-0011)
18. ¿Pueden aceptar una **llave de idempotencia** por operación (crear
    incidente, mensaje, ubicación, videollamada)? Nuestro reintento tras la
    caída de una réplica depende de ello.
19. Si los eventos del C5 llegan por un canal persistente, ¿es posible
    **reanudar desde un cursor** (último evento recibido) tras una desconexión?
    ¿Pueden entregar el mismo evento a varias réplicas, o solo a una?
20. ¿Cada evento del C5 trae un **identificador único global y permanente**
    con el que podamos descartar repetidos? ¿Puede repetirse tras un reinicio
    o una migración del C5? (Hoy deduplicamos por incidente + evento.)
21. ¿Hay una operación para que el ciudadano **termine la conversación** o
    indique que ya está a salvo? ¿Y para avisar que **leyó** los mensajes del
    operador?
22. ¿El catálogo de canales (LSM, chat, audio, traducción, silencioso) existe
    en el C5 o debemos mapearlo a sus tipos de incidente?
23. ¿El C5 conoce las coordenadas de los puntos de entrada (tótems, escuelas)
    por su identificador (`TOT-0142`), o debemos enviarlas nosotros?

## Sesiones no verificadas (ver ADR-0007)
12. ¿Cómo debemos marcar un incidente cuyo QR no se pudo verificar para que el
    operador lo distinga sin dejar de atenderlo?

## Videollamada (sistema del otro equipo)
13b. ¿El sistema soporta llamadas **solo de audio** además de video LSM? La PWA
    ya envía `media: "audio" | "video"` en la solicitud.
13. ¿Quién inicia la videollamada y qué datos necesita el componente para
    abrirse (URL, token, identificador de sala)?
14. ¿Qué identificadores, horas de inicio y fin y metadatos nos pueden
    entregar para nuestro registro?
15. ¿Cómo obtenemos una **referencia a la grabación** (identificador, URL
    firmada, ubicación de almacenamiento) para guardarla en el registro? ¿Por
    cuánto tiempo la conservan?

## Datos y legal
16. ¿El C5 tiene definido un periodo de conservación para estas
    conversaciones? (El nuestro lo define el área jurídica; ver ADR de
    retención en la Fase 2.)
17. ¿Existe un aviso de privacidad institucional del C5 al que debamos
    remitir o que debamos reutilizar?
