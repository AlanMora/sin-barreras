# Punto de integración de la videollamada

La videollamada (LSM) y la llamada de voz las provee el sistema del otro
equipo. Sin Barreras solo **solicita** la llamada y **entrega** los datos de la
sala a su componente.

```
PWA ── video_call_request {media} ──▶ BFF ── C5Gateway.request_video_call ──▶ C5 / sistema de video
PWA ◀── video_call {status: ready, join} ── BFF ◀── evento del C5 (sala lista) ──┘
PWA ── join ──▶ VideoCallProvider (componente del otro equipo)
```

## Qué debe implementar el otro equipo

En `apps/pwa/src/video/provider.ts`:

```ts
export interface VideoCallProvider {
  readonly name: string;
  open(session: {
    join: Record<string, unknown>; // lo que su sistema envió, sin tocar
    media: 'video' | 'audio';
    container: HTMLElement;        // dónde montar su interfaz
    onEnded: () => void;           // avisar al terminar
    language: string;
  }): Promise<{ close(): void }>;
}
```

Y registrarlo al arrancar (`src/main.tsx`):

```ts
registerVideoCallProvider(miProveedor);
```

Hoy está registrado `linkProvider`, que muestra el enlace `room_url` (lo que
devuelve el simulador).

## Requisitos para el componente

- Accesible (WCAG 2.1 AA): controles por teclado, etiquetas, foco visible.
- Pedir cámara y micrófono **solo al entrar a la llamada**, y funcionar (con
  aviso) si se niegan.
- Peso: cargarlo con `import()` dinámico para no afectar el presupuesto de 60 KB.
- Seguridad: si se usa un iframe, agregar su origen a la CSP (`frame-src`).

## Registro
El objeto `join` y lo que el C5 envíe sobre la llamada (ids, horas, referencia
a la grabación) se registran **cifrados** (`video_call_updated`). Ver la
pregunta 15 en `docs/c5-preguntas.md` sobre cómo obtener la referencia a la grabación.
