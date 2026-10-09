/**
 * PUNTO DE INTEGRACIÓN con el sistema de videollamadas (lo construye otro equipo).
 *
 * La PWA solo solicita la llamada (`video_call_request`) y recibe del BFF el
 * objeto `join` cuando la sala está lista. Ese objeto es OPACO: se entrega tal
 * cual al proveedor registrado aquí. Para integrar el componente real basta con
 * implementar `VideoCallProvider` y llamar `registerVideoCallProvider()` en
 * `main.tsx`. Ver docs/videollamada.md.
 */

export interface VideoCallSession {
  /** Datos del sistema de videollamadas (sala, token…), sin interpretar. */
  join: Record<string, unknown>;
  media: 'video' | 'audio';
  /** Contenedor donde el proveedor puede montar su interfaz. */
  container: HTMLElement;
  /** El proveedor debe llamarlo cuando la llamada termine. */
  onEnded: () => void;
  language: string;
}

export interface VideoCallHandle {
  close(): void;
}

export interface VideoCallProvider {
  readonly name: string;
  open(session: VideoCallSession): Promise<VideoCallHandle>;
}

/**
 * Proveedor provisional: muestra un enlace a la sala si el sistema entrega una
 * URL (`room_url`). Se reemplaza al integrar el componente real.
 */
export const linkProvider: VideoCallProvider = {
  name: 'enlace',
  async open({ join, container }) {
    const url = typeof join.room_url === 'string' ? join.room_url : null;
    container.replaceChildren();
    if (url && /^https:\/\//.test(url)) {
      const link = document.createElement('a');
      link.href = url;
      link.target = '_blank';
      link.rel = 'noopener noreferrer';
      link.textContent = container.dataset.label ?? 'Abrir';
      link.className = 'sb-call-link';
      container.append(link);
    }
    return { close: () => container.replaceChildren() };
  },
};

let current: VideoCallProvider = linkProvider;

export function registerVideoCallProvider(provider: VideoCallProvider): void {
  current = provider;
}

export function videoCallProvider(): VideoCallProvider {
  return current;
}
