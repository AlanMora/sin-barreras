# Proyecto: Sin Barreras — PWA ciudadana + BFF + registro completo

## Tu rol
Actúa como ingeniero de software senior especializado en sistemas críticos de tiempo real,
accesibilidad web, diseño de interfaces, seguridad de datos y Rust. Vas a construir conmigo,
desde cero, la parte ciudadana de un sistema de emergencias. Comunícate conmigo siempre en
español: explicaciones, planes, resúmenes y comentarios en el código. Los nombres de
variables, funciones y archivos van en inglés, siguiendo las convenciones de cada lenguaje.

## Contexto del negocio
"Sin Barreras" es el Programa de Atención Incluyente de Emergencias del Gobierno de
Zapopan, operado por el C5. Permite pedir ayuda a personas que no pueden hacerlo por
una llamada tradicional: personas sordas (Lengua de Señas Mexicana), con discapacidad
visual o motriz, personas mayores, niñas y niños, personas que no pueden hablar en ese
momento y personas extranjeras.

El ciudadano escanea un código QR (en tótems, escuelas, hospitales, transporte),
se abre una PWA sin instalar nada, elige cómo comunicarse y queda conectado con un
operador del C5.

Es un sistema de emergencias: un mensaje perdido o una pantalla que no carga puede
costar una vida. La confiabilidad está por encima de cualquier otra prioridad.

Además, nuestro sistema debe conservar un registro completo e íntegro de todo lo que
ocurre del lado ciudadano, como respaldo legal y operativo ante cualquier incidencia,
queja o investigación, independientemente de lo que guarden el C5 o el sistema de
videollamadas.

## Alcance

### Lo que construimos
1. **PWA ciudadana**: la única interfaz que usa el ciudadano.
2. **BFF (Backend For Frontend)** en Rust: la única puerta pública entre la PWA y el C5.
3. **record-writer** en Rust: guarda el registro completo (mensajes, eventos, ubicación,
   metadatos) y las métricas del programa en Postgres.
4. **Infraestructura** para desplegar todo en Docker Swarm.

### Lo que YA EXISTE y NO debemos construir ni modificar
- Consola de operadores del C5 (frontend y backend).
- API y socket del C5 que reciben incidentes y mensajes.
- Sistema de videollamadas (lo hace otro equipo; nosotros solo lo detonamos).
- Tótems físicos (solo se les agrega el QR).

## Arquitectura

Celular → Traefik (TLS) → Nginx (sirve la PWA estática)
                        → BFF Rust (WebSocket + REST) → API/socket C5 (existente)
                                   ↓
                                 Redis (sesiones, rate limit, pub/sub, Redis Streams)
                                   ↓
                          record-writer Rust → Postgres primario → Postgres réplica

- La PWA nunca habla directo con el C5. Todo pasa por el BFF.
- Todo lo que pasa por el BFF, en ambas direcciones, se publica como evento en
  Redis Streams para que `record-writer` lo guarde de forma permanente.
- Redis guarda además el estado temporal: sesiones, rate limit y la cola de mensajes
  pendientes por reconexión.
- El BFF debe ser stateless y correr con mínimo 2 réplicas. El estado compartido
  entre réplicas va en Redis (pub/sub para enrutar mensajes a la réplica donde
  está conectado cada ciudadano).

## Stack

### PWA
- Preact + Vite (`@preact/preset-vite`), TypeScript estricto.
- Estado con `@preact/signals`. Rutas con `preact-iso`.
- `vite-plugin-pwa` para service worker y carga offline del shell.
- Tailwind CSS con design tokens propios basados en la identidad de la campaña:
  azul marino institucional, naranja de acento, tipografía redonda y bold en
  titulares, bordes redondeados. Diseño de nivel profesional, limpio y moderno.
- Íconos SVG de Lucide o Tabler, importados uno por uno.
- Animaciones solo con CSS, respetando `prefers-reduced-motion`.
- Evita `preact/compat` salvo que una librería lo justifique; si la usas, explícame por qué.
- Componentes accesibles construidos a mano siguiendo los patrones de WAI-ARIA
  Authoring Practices, ya que no usaremos librerías de componentes de React.
- i18n con una librería ligera: español por defecto, inglés como segundo idioma,
  estructura lista para más idiomas.
- Presupuesto de rendimiento: bundle inicial menor a 60 KB comprimido; pantalla
  principal usable en menos de 2 segundos en 3G lenta. Falla el build si se excede.

### BFF
- Rust estable, `axum`, `tokio`, `tower` / `tower-http`.
- WebSocket nativo de axum para la comunicación en tiempo real con la PWA.
- `serde` / `serde_json`, `thiserror` para errores, `tracing` + OpenTelemetry
  para logs y trazas.
- Cliente de Redis: `fred` (o `redis-rs` si hay una razón clara; explícamela).
- Configuración por variables de entorno; secretos vía Docker secrets.

### record-writer
- Rust estable, `tokio`, `sqlx` con migraciones versionadas.
- Consume eventos de Redis Streams con consumer groups.

### Infraestructura
- Dockerfiles multi-stage (imagen final mínima: distroless o scratch para Rust,
  nginx-alpine para la PWA).
- `docker-stack.yml` para Swarm con labels de Traefik, healthchecks,
  2 réplicas por servicio sin estado, límites de recursos y `update_config` con rollback.
- Redis con persistencia AOF activada, para que los eventos pendientes de guardar
  sobrevivan a un reinicio.
- Postgres primario y réplica fijados a nodos distintos con placement constraints
  y volúmenes locales.
- `docker-compose.yml` para desarrollo local.

### Estructura del repositorio
/apps/pwa             → Preact
/apps/bff             → Rust
/apps/record-writer   → Rust
/apps/c5-mock         → simulador de la API del C5 para desarrollo
/infra                → Dockerfiles, stack de Swarm, configs de Nginx, Traefik, Redis y Postgres
/docs                 → ADRs, contrato de eventos, modelo de datos, decisiones

Los servicios de Rust comparten un cargo workspace (propón la mejor ubicación).

## Integración con el C5 (IMPORTANTE)
Todavía NO tengo la documentación de la API ni del socket del C5.
- No inventes endpoints del C5 como si fueran reales.
- En el BFF define un trait (puerto) `C5Gateway` con las operaciones que necesitamos:
  crear incidente, enviar mensaje, recibir mensajes del operador, recibir cambios de
  estado, actualizar ubicación y solicitar videollamada.
- Implementa ese trait con un adaptador simulado (`c5-mock`) que se comporte como un
  operador: responde mensajes, cambia estados y simula latencia y desconexiones.
- Cuando llegue la documentación real, solo se escribirá un nuevo adaptador.
- Guarda el identificador del incidente que devuelva el C5 junto con nuestro propio
  identificador, para poder cruzar ambos registros en cualquier investigación.
- Deja en `/docs/c5-preguntas.md` la lista de dudas que surjan para el equipo del C5.

## Contrato PWA ↔ BFF
Diseña y documenta en `/docs/contrato-eventos.md` un protocolo de mensajes JSON por
WebSocket con estas reglas:
- Cada mensaje del cliente lleva `client_msg_id` (UUID) para idempotencia: si se
  reenvía por reconexión, el BFF no lo duplica.
- Cada mensaje del servidor lleva un `seq` incremental por sesión. Al reconectar, la
  PWA envía su último `seq` recibido y el BFF reenvía lo que falte.
- ACK explícito de cada mensaje en ambas direcciones.
- Eventos mínimos: inicio de sesión, crear incidente (con canal elegido), mensaje de
  chat, estado del incidente, actualización de ubicación, solicitud de videollamada,
  heartbeat/ping, error y cierre.
- Versión del protocolo en el handshake.

## Funcionalidades de la PWA

1. **Entrada por QR**: la URL trae el identificador del punto, por ejemplo
   `/?t=TOT-0142&s=<firma>`. La firma es un HMAC que valida el BFF para evitar QR
   falsificados. Ese punto sirve como ubicación de respaldo inmediata.
2. **Aviso de privacidad**: visible y accesible, breve, que informe que la conversación
   y la ubicación se registran para la atención y el respaldo de la emergencia.
   NUNCA debe bloquear ni retrasar la solicitud de ayuda: se muestra de forma no
   intrusiva y el ciudadano puede pedir ayuda de inmediato.
3. **Pantalla "¿Cómo deseas comunicarte?"** con 5 opciones grandes:
   videollamada LSM, chat de texto, audio, traducción a otro idioma y modo silencioso.
4. **Chat en tiempo real** con indicadores de enviado, entregado y leído, y
   estado visible de la conexión.
5. **Modo silencioso**: sin sonidos ni vibración, brillo y colores discretos,
   botones de respuesta rápida ("estoy en peligro", "no puedo hablar",
   "alguien me sigue", "estoy herido") y un botón de salida rápida que lleve a una
   pantalla neutral.
6. **Videollamada**: la PWA solo envía la solicitud y recibe los datos para abrir el
   componente del otro equipo. Deja un punto de integración claro y documentado.
7. **Estado del incidente**: "te estamos atendiendo", "unidad en camino",
   tiempo estimado, "incidente cerrado".
8. **Ajustes**: idioma, tamaño de letra y alto contraste.

## Registro completo y métricas (persistencia propia)

### Qué se guarda: TODO
Nuestro sistema es respaldo independiente ante cualquier incidencia. Se guarda de
forma permanente, aunque la información también exista en el C5 o en el sistema de
videollamadas:
- **Todos los mensajes**, en ambas direcciones (ciudadano → operador y operador →
  ciudadano), con su contenido completo, incluidas las respuestas rápidas del modo
  silencioso.
- **Todos los eventos** del ciclo de vida: apertura de la PWA, punto de entrada
  (QR/tótem/web), aviso de privacidad mostrado, canal elegido, incidente creado,
  primera respuesta del operador, solicitud de videollamada, cambios de estado,
  cierre, abandono antes de pedir ayuda, desconexiones y reconexiones, errores.
- **Ubicación exacta** con todo su historial: la del QR y cada actualización de GPS,
  con precisión reportada y marca de tiempo.
- **Metadatos técnicos**: IP, user agent, idioma, ajustes de accesibilidad usados,
  versión de la app y del protocolo.
- **Referencias de la videollamada**: identificadores, hora de inicio y fin, y
  cualquier dato que nos entregue el sistema del otro equipo (el video en sí lo
  guarda ese sistema; deja documentado en `/docs/c5-preguntas.md` cómo obtener la
  referencia a la grabación).
- **Identificadores cruzados**: nuestro ID de sesión e incidente y el ID del C5.
- Cada registro con marca de tiempo del servidor (UTC, precisión de milisegundos)
  y, cuando aplique, la del cliente.

### Flujo y garantía de no pérdida
BFF → Redis Streams (con AOF) → `record-writer` → Postgres primario → réplica

- REGLA DE ORO 1: guardar nunca debe bloquear ni afectar el flujo de emergencia.
  El BFF solo publica eventos en Redis Streams (operación no bloqueante). Si
  Postgres no está disponible, los eventos esperan en el stream y se guardan cuando
  se recupere.
- REGLA DE ORO 2: no se pierde ningún registro. Consumer groups con ACK solo después
  de confirmar la escritura en Postgres, reintentos, manejo de mensajes pendientes
  (`XPENDING` / `XCLAIM`) y una cola de errores para eventos que fallen
  repetidamente. Las escrituras son idempotentes para no duplicar.
- Postgres en primario + réplica de streaming. Las escrituras van al primario; los
  tableros, reportes y consultas de investigación leen de la réplica.
- Respaldos con pgBackRest: completos e incrementales, cifrados, con copia fuera
  del servidor, y una prueba de restauración documentada.
- Documenta el modelo de datos en `/docs/modelo-datos.md`.

## Protección de los datos
Se guardan datos personales sensibles (conversaciones de emergencia de personas con
discapacidad, ubicación exacta). Guardarlo todo exige protegerlo al máximo:

- **Cifrado en tránsito**: TLS en todo, incluido el tráfico interno hacia Postgres
  y Redis.
- **Cifrado en reposo**: el contenido de los mensajes, la ubicación exacta y la IP se
  cifran a nivel de aplicación (AES-256-GCM) antes de guardarse. Las llaves viven en
  Docker secrets, nunca en el código ni en la base de datos. Diseña la rotación de
  llaves y regístrala en un ADR.
- **Integridad y valor probatorio**: el registro es de solo inserción (append-only).
  Ningún usuario de la aplicación puede modificar ni borrar registros; aplícalo con
  permisos de Postgres, no solo con código. Cada registro incluye un hash encadenado
  con el anterior (hash chain) para detectar cualquier alteración, y debe existir una
  herramienta para verificar la integridad de la cadena.
- **Control de acceso**: roles de Postgres separados (escritura para `record-writer`,
  solo lectura para reportes, sin acceso directo para nadie más). Mínimo privilegio.
- **Bitácora de accesos**: toda consulta a registros sensibles queda registrada:
  quién, cuándo y qué consultó.
- **Logs**: los logs de aplicación nunca contienen el contenido de mensajes, ubicación
  ni IP; eso solo vive cifrado en la base de datos.
- **Retención**: el periodo de conservación se define con el área jurídica del
  municipio. Déjalo configurable y documentado en un ADR, sin borrado automático
  hasta que esté definido.
- Cumplimiento de la LFPDPPP y la normativa aplicable a sujetos obligados (datos
  sensibles, aviso de privacidad, derechos ARCO).

## Requisitos no funcionales

### Confiabilidad (prioridad máxima)
- Reconexión automática con backoff exponencial y jitter.
- Cola local de mensajes pendientes: ningún "necesito ayuda" se pierde.
- Manejo explícito de iOS/Safari, que suspende WebSockets al bloquear la pantalla o
  cambiar de app: al volver al foco, reconectar y recuperar mensajes por `seq`.
- Ubicación: enviar primero la ubicación del QR, después la del GPS cuando llegue,
  y actualizarla periódicamente. La app debe funcionar aunque nieguen el permiso.
- Los permisos (ubicación, cámara, micrófono) se piden solo en el momento en que se
  necesitan, y la app sigue funcionando si los niegan.
- Una falla de Postgres o de `record-writer` nunca debe afectar al ciudadano, y
  nunca debe causar pérdida de registros.

### Accesibilidad (requisito, no extra)
- WCAG 2.1 nivel AA como mínimo.
- Totalmente usable con TalkBack y VoiceOver; regiones `aria-live` para mensajes
  nuevos y cambios de estado.
- Navegación completa por teclado, foco visible, objetivos táctiles de al menos 48 px.
- Lenguaje simple y claro en todos los textos.

### Seguridad
- Sin cuentas: el BFF emite un token anónimo de sesión de corta duración.
- Rate limiting por IP y por sesión en Redis, sin bloquear a usuarios legítimos en
  una emergencia real (documenta la estrategia y sus umbrales).
- Validación estricta de todo lo que entra al BFF.
- CORS, CSP y cabeceras de seguridad bien configuradas.

### Observabilidad
- Trazas OpenTelemetry de punta a punta (PWA → BFF → C5).
- Métricas técnicas: conexiones activas, latencia de mensajes, reconexiones, errores
  hacia el C5, rechazos por rate limit, eventos pendientes en el stream, eventos en
  cola de errores y tiempo de retraso de `record-writer`.
- Alertas cuando el stream crezca o haya eventos en la cola de errores.
- Endpoints `/health` y `/ready` en el BFF y en `record-writer`.

### Pruebas
- BFF y record-writer: pruebas unitarias y de integración (con Redis y Postgres
  reales en contenedor).
- Pruebas específicas de no pérdida: Postgres caído, `record-writer` reiniciado a
  media escritura y Redis reiniciado; verificar que todos los registros llegan
  exactamente una vez.
- Pruebas de cifrado y de verificación de la cadena de integridad.
- PWA: pruebas unitarias de la lógica de conexión y de la cola de mensajes.
- E2E con Playwright, incluyendo escenarios de red caída y reconexión.
- Auditoría automática de accesibilidad con axe-core en los E2E.

## Forma de trabajar
1. **Antes de escribir código**, lee todo este documento y presenta un plan por
   fases con entregables concretos. Espera mi aprobación.
2. Si algo es ambiguo, pregúntame en vez de suponer. Si tomas una decisión técnica
   importante, regístrala como ADR en `/docs/adr/`.
3. Trabaja en fases pequeñas y verificables. Al terminar cada fase: compila, corre
   las pruebas y dame un resumen en español de qué se hizo, cómo probarlo y qué sigue.
4. Commits pequeños con mensajes claros (Conventional Commits).
5. No agregues dependencias sin justificarlas.
6. Si detectas un riesgo para la confiabilidad, la seguridad, la integridad de los
   registros o la accesibilidad, avísame aunque no te lo haya pedido.

## Fases sugeridas (ajústalas en tu plan si ves algo mejor)
0. Estructura del repo, tooling, docker-compose local, CI básica.
1. Contrato de eventos + BFF base (WebSocket, sesiones, Redis) + `c5-mock`.
2. Registro completo: modelo de datos, `record-writer`, Redis Streams con garantía
   de no pérdida, cifrado y cadena de integridad. (Va temprano porque todo lo que
   se construya después debe quedar registrado desde el inicio.)
3. PWA: sistema de diseño (tokens, tipografía, componentes base), entrada por QR,
   aviso de privacidad, selección de canal y chat funcionando contra el mock.
4. Confiabilidad: reconexión, cola de pendientes, recuperación por `seq`, ubicación.
5. Modo silencioso, estados del incidente, punto de integración de videollamada.
6. Accesibilidad completa, i18n y ajustes.
7. Seguridad: firma de QR, rate limit, cabeceras, roles de base de datos y
   bitácora de accesos.
8. Infraestructura de Swarm (Postgres primario + réplica, respaldos cifrados y
   prueba de restauración), observabilidad, alertas y pruebas E2E.

Empieza leyendo este documento y dame tu plan.
