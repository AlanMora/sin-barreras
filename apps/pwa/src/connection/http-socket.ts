/**
 * Transporte HTTP de respaldo con la interfaz de un WebSocket (ADR-0018).
 *
 * El `ConnectionClient` no sabe si habla por WebSocket o por HTTP: este objeto
 * traduce `send()` a `POST /api/v1/hello` y `POST /api/v1/sync` (long-polling)
 * y entrega las respuestas por `onmessage`, con la misma semántica.
 */
import type { ClientMessage, ServerControl, Welcome } from '../protocol/generated';
import type { WebSocketLike } from './client';

const LONG_POLL_MS = 20_000;
/** Cada cierto tiempo se cierra para que se vuelva a intentar WebSocket. */
const MAX_LIFETIME_MS = 5 * 60_000;

type Fetch = typeof fetch;

export class HttpSocket implements WebSocketLike {
  readyState = 0;
  onopen: ((ev: unknown) => void) | null = null;
  onmessage: ((ev: { data: unknown }) => void) | null = null;
  onclose: ((ev: { code: number }) => void) | null = null;
  onerror: ((ev: unknown) => void) | null = null;

  private session: { id: string; token: string } | null = null;
  private lastSeq = 0;
  private queue: ClientMessage[] = [];
  private inFlight: AbortController | null = null;
  private running = false;
  private readonly lifetime: ReturnType<typeof setTimeout>;

  constructor(
    private readonly base: string,
    private readonly fetchImpl: Fetch = (...args) => fetch(...args),
  ) {
    this.lifetime = setTimeout(() => this.close(4001), MAX_LIFETIME_MS);
    queueMicrotask(() => {
      if (this.readyState !== 0) return;
      this.readyState = 1;
      this.onopen?.({});
    });
  }

  send(data: string): void {
    if (this.readyState !== 1) return;
    const message = JSON.parse(data) as ClientMessage;
    if (message.type === 'hello') {
      this.lastSeq = message.resume?.last_seq ?? 0;
      void this.hello(data);
      return;
    }
    if (message.type === 'ping') {
      // El ciclo HTTP vivo es el latido: se responde aquí mismo.
      this.emit({
        type: 'pong',
        client_msg_id: message.client_msg_id,
        server_ts: new Date().toISOString(),
      });
      return;
    }
    this.queue.push(message);
    // Si hay un long-poll esperando, se corta para enviar ya.
    this.inFlight?.abort();
  }

  close(code = 1000): void {
    if (this.readyState === 3) return;
    this.readyState = 3;
    this.running = false;
    clearTimeout(this.lifetime);
    this.inFlight?.abort();
    this.onclose?.({ code });
  }

  private emit(frame: object): void {
    if (this.readyState === 1) this.onmessage?.({ data: JSON.stringify(frame) });
  }

  private async post(path: string, body: string, signal?: AbortSignal): Promise<Response> {
    return this.fetchImpl(`${this.base}${path}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body,
      ...(signal ? { signal } : {}),
    });
  }

  private closeFor(status: number): void {
    if (status === 401) this.close(4410);
    else if (status === 426) this.close(4426);
    else this.close(4503);
  }

  private async hello(body: string): Promise<void> {
    try {
      const response = await this.post('/api/v1/hello', body);
      const json = (await response.json()) as {
        welcome?: ServerControl;
        frames?: { seq: number }[];
      } & ServerControl;
      if (!response.ok) {
        this.emit(json);
        this.closeFor(response.status);
        return;
      }
      const welcome = json.welcome as Welcome & { type: 'welcome' };
      this.session = { id: welcome.session_id, token: welcome.session_token };
      this.emit(welcome);
      this.deliver(json.frames ?? []);
      this.running = true;
      void this.loop();
    } catch {
      this.close(1006);
    }
  }

  private deliver(frames: { seq: number }[]): void {
    for (const frame of frames) {
      this.lastSeq = Math.max(this.lastSeq, frame.seq);
      this.emit(frame);
    }
  }

  private async loop(): Promise<void> {
    while (this.running && this.session) {
      const commands = this.queue.splice(0);
      const controller = new AbortController();
      this.inFlight = controller;
      try {
        const response = await this.post(
          '/api/v1/sync',
          JSON.stringify({
            session_id: this.session.id,
            session_token: this.session.token,
            last_seq: this.lastSeq,
            commands,
            wait_ms: commands.length > 0 ? 0 : LONG_POLL_MS,
          }),
          controller.signal,
        );
        if (!response.ok) {
          this.closeFor(response.status);
          return;
        }
        const json = (await response.json()) as {
          replies: ServerControl[];
          frames: { seq: number }[];
        };
        for (const reply of json.replies) this.emit(reply);
        this.deliver(json.frames);
      } catch {
        if (controller.signal.aborted && this.running) {
          // Corte intencional para enviar comandos: los no enviados vuelven a la cola.
          this.queue.unshift(...commands);
          continue;
        }
        this.close(1006);
        return;
      } finally {
        this.inFlight = null;
      }
    }
  }
}

/**
 * Elige el transporte: WebSocket mientras funcione; tras 3 fallas seguidas al
 * abrirlo, HTTP. Mientras se usa HTTP, cada 4.º intento vuelve a probar
 * WebSocket por si la red ya lo permite.
 */
export function transportFactory(wsUrl: string, httpBase: string): (url: string) => WebSocketLike {
  let wsFailures = 0;
  let httpAttempts = 0;
  return () => {
    if (wsFailures >= 3) {
      httpAttempts += 1;
      if (httpAttempts % 4 !== 0) return new HttpSocket(httpBase);
    }
    const ws = new WebSocket(wsUrl);
    let opened = false;
    ws.addEventListener('open', () => {
      opened = true;
      wsFailures = 0;
      httpAttempts = 0;
    });
    ws.addEventListener('close', () => {
      if (!opened) wsFailures += 1;
    });
    return ws as unknown as WebSocketLike;
  };
}
