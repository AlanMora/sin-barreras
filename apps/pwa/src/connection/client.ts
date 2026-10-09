/**
 * Cliente del protocolo v1 (ver docs/contrato-eventos.md).
 *
 * Garantías hacia el ciudadano:
 * - **Ningún mensaje se pierde:** todo comando entra primero a la cola local
 *   persistente y solo sale de ella con el `ack` del servidor. Se reenvía
 *   (mismo `client_msg_id`) en cada reconexión; el BFF descarta duplicados.
 * - **Nada se recibe dos veces ni en desorden:** los mensajes del servidor se
 *   entregan por `seq` contiguo; los que llegan adelantados esperan en un búfer.
 * - **Reconexión automática** con backoff exponencial y jitter; inmediata al
 *   volver la red o el foco (iOS suspende el WebSocket al bloquear la pantalla).
 * - **Conexiones zombis:** ping cada 15 s; si el servidor calla, se reconecta.
 */
import { nowIso, traceparent, uuid } from '../lib/ids';
import type {
  AccessibilitySettings,
  AckStatus,
  ClientMessage,
  QrEntry,
  SequencedMessage,
  ServerControl,
  ServerError,
  ServerFrame,
  Welcome,
} from '../protocol/generated';
import { HEARTBEAT_INTERVAL_MS, PROTOCOL_VERSION } from '../protocol/generated';
import { Backoff } from './backoff';
import type { OutboxStore, QueuedCommand, StoredSession } from './storage';

export type ConnectionStatus = 'connecting' | 'online' | 'reconnecting' | 'offline' | 'outdated';

/** Comandos que la app puede enviar (el cliente agrega id, hora y traza). */
type Omitted = 'client_msg_id' | 'client_ts' | 'traceparent';
type DistributiveOmit<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never;
export type OutgoingCommand = DistributiveOmit<
  Exclude<ClientMessage, { type: 'hello' } | { type: 'ack' } | { type: 'ping' }>,
  Omitted
>;

export interface ConnectionEvents {
  onStatus(status: ConnectionStatus): void;
  /** `newSession` = el servidor no pudo reanudar: la conversación empieza de cero. */
  onWelcome(welcome: Welcome, newSession: boolean): void;
  onEvent(message: SequencedMessage): void;
  onAck(clientMsgId: string, status: AckStatus): void;
  onRejected(clientMsgId: string, error: ServerError): void;
}

export interface WebSocketLike {
  readonly readyState: number;
  send(data: string): void;
  close(code?: number, reason?: string): void;
  onopen: ((ev: unknown) => void) | null;
  onmessage: ((ev: { data: unknown }) => void) | null;
  onclose: ((ev: { code: number }) => void) | null;
  onerror: ((ev: unknown) => void) | null;
}

export interface HelloInfo {
  appVersion: string;
  language: string;
  accessibility: AccessibilitySettings;
  entry: QrEntry | null;
}

export interface ClientDeps {
  url: string;
  hello: () => HelloInfo;
  store: OutboxStore;
  sessions: {
    load(): StoredSession | null;
    save(s: StoredSession): void;
    clear(): void;
  };
  events: ConnectionEvents;
  createSocket?: (url: string) => WebSocketLike;
  isOnline?: () => boolean;
  random?: () => number;
  now?: () => number;
}

const OPEN = 1;
const HELLO_TIMEOUT_MS = 10_000;
/** Si el servidor no dice nada en este tiempo, la conexión está muerta. */
const STALE_AFTER_MS = HEARTBEAT_INTERVAL_MS * 2 + 5_000;
const RETRY_REJECTED_MS = 2_000;

export class ConnectionClient {
  private ws: WebSocketLike | null = null;
  private established = false;
  private stopped = false;
  private status: ConnectionStatus = 'connecting';
  private readonly outbox = new Map<string, QueuedCommand>();
  private order = 0;
  /** Mayor `seq` contiguo entregado a la app. */
  private lastSeq = 0;
  private readonly ahead = new Map<number, SequencedMessage>();
  private readonly backoff: Backoff;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  private helloTimer: ReturnType<typeof setTimeout> | null = null;
  private retryTimer: ReturnType<typeof setTimeout> | null = null;
  private lastServerActivity = 0;
  private resumeRequested = false;

  constructor(private readonly deps: ClientDeps) {
    this.backoff = new Backoff(500, 15_000, deps.random ?? Math.random);
  }

  private now(): number {
    return this.deps.now?.() ?? Date.now();
  }

  private online(): boolean {
    return this.deps.isOnline?.() ?? true;
  }

  get pendingCount(): number {
    return this.outbox.size;
  }

  isPending(clientMsgId: string): boolean {
    return this.outbox.has(clientMsgId);
  }

  /** Carga la cola persistida y conecta. */
  async start(): Promise<void> {
    try {
      for (const queued of await this.deps.store.load()) {
        this.outbox.set(queued.message.client_msg_id, queued);
        this.order = Math.max(this.order, queued.order + 1);
      }
    } catch {
      // Sin almacenamiento: se continúa solo con memoria.
    }
    this.connect();
  }

  stop(): void {
    this.stopped = true;
    this.clearTimers();
    this.ws?.close(1000, 'stop');
    this.ws = null;
  }

  /**
   * Encola un comando y lo envía si hay conexión. Devuelve su `client_msg_id`.
   * El comando NO se descarta hasta recibir `ack`.
   */
  send(command: OutgoingCommand, clientMsgId: string = uuid()): string {
    const message = {
      ...command,
      client_msg_id: clientMsgId,
      client_ts: nowIso(),
      ...(command.type === 'client_event' || command.type === 'close'
        ? {}
        : { traceparent: traceparent() }),
    } as ClientMessage;
    const queued: QueuedCommand = { order: this.order++, message };
    this.outbox.set(clientMsgId, queued);
    this.deps.store.put(queued).catch(() => undefined);
    if (this.established) this.transmit(message);
    return clientMsgId;
  }

  /**
   * Llamar cuando vuelve la red, la pestaña se vuelve visible o la página se
   * restaura (bfcache). En iOS el WebSocket puede estar muerto sin aviso.
   */
  wake(): void {
    if (this.stopped || this.status === 'outdated') return;
    const stale = this.now() - this.lastServerActivity > STALE_AFTER_MS;
    if (!this.ws || this.ws.readyState !== OPEN || !this.established || stale) {
      this.reconnectNow();
    } else {
      this.ping();
    }
  }

  /** La red se cayó (evento `offline` del navegador). */
  wentOffline(): void {
    this.setStatus('offline');
  }

  private setStatus(status: ConnectionStatus): void {
    if (this.status === status) return;
    this.status = status;
    this.deps.events.onStatus(status);
  }

  private connect(): void {
    if (this.stopped) return;
    this.clearTimer('reconnectTimer');
    this.established = false;
    this.setStatus(
      this.lastServerActivity === 0 ? 'connecting' : this.online() ? 'reconnecting' : 'offline',
    );
    let socket: WebSocketLike;
    try {
      socket = this.deps.createSocket
        ? this.deps.createSocket(this.deps.url)
        : (new WebSocket(this.deps.url) as unknown as WebSocketLike);
    } catch {
      this.scheduleReconnect();
      return;
    }
    this.ws = socket;
    socket.onopen = () => this.onOpen(socket);
    socket.onmessage = (ev) => this.onMessage(socket, ev.data);
    socket.onclose = (ev) => this.onClose(socket, ev.code);
    socket.onerror = () => {
      // `onclose` llega después y decide.
    };
  }

  private onOpen(socket: WebSocketLike): void {
    if (socket !== this.ws) return;
    const info = this.deps.hello();
    const session = this.deps.sessions.load();
    this.resumeRequested = session !== null;
    const hello: ClientMessage = {
      type: 'hello',
      client_msg_id: uuid(),
      client_ts: nowIso(),
      traceparent: traceparent(),
      protocol_version: PROTOCOL_VERSION,
      app_version: info.appVersion,
      language: info.language,
      accessibility: info.accessibility,
      ...(info.entry ? { entry: info.entry } : {}),
      ...(session ? { resume: { ...session, last_seq: this.lastSeq } } : {}),
    };
    socket.send(JSON.stringify(hello));
    this.helloTimer = setTimeout(() => socket.close(4000, 'sin welcome'), HELLO_TIMEOUT_MS);
  }

  private onMessage(socket: WebSocketLike, data: unknown): void {
    if (socket !== this.ws || typeof data !== 'string') return;
    this.lastServerActivity = this.now();
    let frame: ServerFrame;
    try {
      frame = JSON.parse(data) as ServerFrame;
    } catch {
      return;
    }
    if ('seq' in frame && typeof frame.seq === 'number') {
      this.onSequenced(frame as SequencedMessage);
    } else {
      this.onControl(frame as ServerControl);
    }
  }

  private onControl(frame: ServerControl): void {
    switch (frame.type) {
      case 'welcome':
        this.onWelcome(frame);
        break;
      case 'ack':
        this.removeFromOutbox(frame.client_msg_id);
        this.deps.events.onAck(frame.client_msg_id, frame.status);
        break;
      case 'error':
        this.onError(frame);
        break;
      case 'pong':
        break;
    }
  }

  private onWelcome(welcome: Welcome): void {
    this.clearTimer('helloTimer');
    const newSession = this.resumeRequested ? !welcome.resumed : true;
    if (newSession) {
      this.lastSeq = 0;
      this.ahead.clear();
    }
    this.deps.sessions.save({
      session_id: welcome.session_id,
      session_token: welcome.session_token,
    });
    this.established = true;
    this.backoff.reset();
    this.setStatus('online');
    this.deps.events.onWelcome(welcome, newSession);
    this.startHeartbeat();
    this.flush();
  }

  private onError(error: ServerError): void {
    if (error.code === 'unsupported_protocol_version') {
      this.setStatus('outdated');
      this.stopped = true;
      return;
    }
    const id = error.client_msg_id;
    if (!id || !this.outbox.has(id)) return;
    if (error.retryable) {
      // Se conserva en la cola; se reintenta en breve.
      this.clearTimer('retryTimer');
      this.retryTimer = setTimeout(() => this.flush(), RETRY_REJECTED_MS);
    } else {
      this.removeFromOutbox(id);
      this.deps.events.onRejected(id, error);
    }
  }

  private onSequenced(message: SequencedMessage): void {
    if (message.seq <= this.lastSeq) {
      // Duplicado (reenvío tras reconectar): se confirma de nuevo y se ignora.
      this.sendAck(message.seq);
      return;
    }
    this.ahead.set(message.seq, message);
    let next = this.ahead.get(this.lastSeq + 1);
    while (next) {
      this.ahead.delete(next.seq);
      this.lastSeq = next.seq;
      this.deps.events.onEvent(next);
      this.sendAck(next.seq);
      next = this.ahead.get(this.lastSeq + 1);
    }
  }

  private sendAck(seq: number): void {
    if (this.ws?.readyState !== OPEN) return;
    this.ws.send(JSON.stringify({ type: 'ack', client_msg_id: uuid(), seq }));
  }

  private onClose(socket: WebSocketLike, code: number): void {
    if (socket !== this.ws) return;
    this.ws = null;
    this.established = false;
    this.clearTimer('heartbeatTimer');
    this.clearTimer('helloTimer');
    if (code === 4426) {
      this.setStatus('outdated');
      this.stopped = true;
      return;
    }
    if (code === 4410) {
      // La sesión expiró: nueva sesión de inmediato; la cola se reenvía en ella.
      this.deps.sessions.clear();
      this.reconnectNow();
      return;
    }
    this.scheduleReconnect();
  }

  private flush(): void {
    if (!this.established) return;
    for (const queued of [...this.outbox.values()].sort((a, b) => a.order - b.order)) {
      this.transmit(queued.message);
    }
  }

  private transmit(message: ClientMessage): void {
    if (this.ws?.readyState === OPEN) this.ws.send(JSON.stringify(message));
  }

  private removeFromOutbox(clientMsgId: string): void {
    if (this.outbox.delete(clientMsgId)) {
      this.deps.store.remove(clientMsgId).catch(() => undefined);
    }
  }

  private startHeartbeat(): void {
    this.clearTimer('heartbeatTimer');
    this.heartbeatTimer = setInterval(() => {
      if (this.now() - this.lastServerActivity > STALE_AFTER_MS) {
        // El servidor no responde: forzar reconexión.
        this.ws?.close(4000, 'sin respuesta');
        return;
      }
      this.ping();
    }, HEARTBEAT_INTERVAL_MS);
  }

  private ping(): void {
    this.transmitRaw({ type: 'ping', client_msg_id: uuid(), client_ts: nowIso() });
  }

  private transmitRaw(message: ClientMessage): void {
    if (this.ws?.readyState === OPEN) this.ws.send(JSON.stringify(message));
  }

  private reconnectNow(): void {
    const old = this.ws;
    this.ws = null;
    this.established = false;
    if (old) {
      old.onclose = null;
      old.onmessage = null;
      old.close(4001, 'reconectando');
    }
    this.clearTimer('heartbeatTimer');
    this.clearTimer('helloTimer');
    this.backoff.reset();
    this.connect();
  }

  private scheduleReconnect(): void {
    if (this.stopped) return;
    this.setStatus(this.online() ? 'reconnecting' : 'offline');
    this.clearTimer('reconnectTimer');
    this.reconnectTimer = setTimeout(() => this.connect(), this.backoff.next());
  }

  private clearTimer(
    name: 'reconnectTimer' | 'heartbeatTimer' | 'helloTimer' | 'retryTimer',
  ): void {
    const timer = this[name];
    if (timer !== null) {
      clearTimeout(timer);
      clearInterval(timer);
      this[name] = null;
    }
  }

  private clearTimers(): void {
    this.clearTimer('reconnectTimer');
    this.clearTimer('heartbeatTimer');
    this.clearTimer('helloTimer');
    this.clearTimer('retryTimer');
  }
}
