# Accesibilidad

Objetivo: WCAG 2.1 nivel AA como mínimo, usable con TalkBack y VoiceOver.

## Automatizado (en CI)

| Verificación | Dónde |
|---|---|
| axe-core, reglas WCAG 2.0/2.1 A y AA, en las 8 pantallas | `apps/pwa/e2e/accessibility.spec.ts` |
| axe con alto contraste y letra al 200 % | ídem |
| axe en inglés (`lang="en"`) | ídem |
| Navegación por teclado (saltar al contenido, elegir canal, foco al título) | ídem |
| Objetivos táctiles ≥ 48 px | ídem |
| Reglas de accesibilidad de Biome en nivel `error` | `biome.json` |
| Contraste de las 3 paletas calculado y documentado | `apps/pwa/src/styles.css` |

## Decisiones

- **Una sola región `aria-live`** (cortés) para mensajes nuevos del C5 y cambios
  de estado. La lista de mensajes NO es región viva: así el lector no repite los
  mensajes propios. El estado de la conexión usa `role="status"`.
- **Foco al título** al cambiar de pantalla; en la carga inicial no se mueve
  (el primer Tab llega a "Saltar al contenido").
- **Zoom nunca bloqueado**; el tamaño de letra escala toda la interfaz (`rem`).
- **Alto contraste** propio (negro/blanco/amarillo, ≥ 14:1) además del del sistema.
- **Movimiento reducido** respetado globalmente.
- Controles nativos (radios, casillas, enlaces, botones) con estilos propios de
  contraste suficiente en las tres paletas.
- Lenguaje simple: frases cortas, segunda persona, sin tecnicismos.

## Verificación manual pendiente (no automatizable)

Debe hacerla una persona antes de producción, idealmente con usuarios de la
comunidad sorda, ciega y de personas mayores:

- [ ] **TalkBack** (Android, Chrome): recorrer las 8 pantallas; pedir ayuda por chat;
      escuchar el anuncio de un mensaje nuevo del C5 y de "unidad en camino".
- [ ] **VoiceOver** (iOS, Safari): ídem; bloquear la pantalla 2 minutos y volver
      (debe reconectar y anunciar lo que llegó).
- [ ] Modo silencioso con VoiceOver: confirmar que la salida rápida no anuncia
      nada que delate la emergencia.
- [ ] Control por interruptor / acceso con un botón (Android Switch Access).
- [ ] Letra del sistema al máximo + "Muy grande" en la app.
- [ ] Validar con intérpretes de LSM el flujo de videollamada (cuando exista el
      componente del otro equipo).
