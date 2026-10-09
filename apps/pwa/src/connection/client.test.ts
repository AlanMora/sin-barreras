import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ClientMessage, SequencedMessage, Welcome } from '../protocol/generated';
import { Backoff } from './backoff';
import {
  ConnectionClient,
  type ConnectionEvents,
  type ConnectionStatus,
  type WebSocketLike,
} from './client';
import { MemoryOutboxStore, type StoredSession } from './storage';

class FakeSocket implements WebSocketLike {
  static all: FakeSocket[] = [];
  readyState = 0;
  sent: ClientMessage[] = [];
  onopen: ((ev: unknown) => void) | null = null;
  onmessage: ((ev: { data: unknown }) => void) | null = null;
  onclose: ((ev: { code: number }) => void) | null = null;
  onerror: ((ev: unknown) => void) | null = null;

  constructor() {
    FakeSocket.all.push(this);
  }
  send(data: string) {
    this.sent.push(JSON.parse(data) as ClientMessage);
  }
  close(code = 1000) {
    this.readyState = 3;
    this.onclose?.({ code });
  }
  // Acciones del "servidor" en la prueba:
  open() {
    this.readyState = 1;
    this.onopen?.({});
  }
  receive(frame: object) {
    this.onmessage?.({ data: JSON.stringify(frame) });
  }
  drop(code = 1006) {
    this.readyState = 3;
    this.onclose?.({ code });
  }
  sentOf(type: string) {
    return this.sent.filter((m) => m.type === type);
  }
}

const latest = () => FakeSocket.all[FakeSocket.all.length - 1] as FakeSocket;

function welcome(overrides: Partial<Welcome> = {}): Welcome & { type: 'welcome' } {
  return {
    type: 'welcome',
    protocol_version: 1,
    session_id: 's-1',
    session_token: 'tok-1',
    token_expires_at: '2026-10-09T20:00:00.000Z',
    resumed: false,
    entry: 'none',
    heartbeat_interval_ms: 15_000,
    latest_seq: 0,
    server_ts: '2026-10-09T19:00:00.000Z',
    ...overrides,
  };
}

function chat(seq: number, text = `m${seq}`): SequencedMessage {
  return {
    seq,
    type: 'chat_message',
    message_id: `id-${seq}`,
    author: 'operator',
    kind: 'text',
    text,
    server_ts: '2026-10-09T19:00:00.000Z',
  } as SequencedMessage;
}

function setup(opts: { store?: MemoryOutboxStore; session?: StoredSession | null } = {}) {
  let saved: StoredSession | null = opts.session ?? null;
  const statuses: ConnectionStatus[] = [];
  const events: SequencedMessage[] = [];
  const acks: string[] = [];
  const rejected: string[] = [];
  const welcomes: { welcome: Welcome; newSession: boolean }[] = [];
  const handlers: ConnectionEvents = {
    onStatus: (s) => statuses.push(s),
    onWelcome: (w, n) => welcomes.push({ welcome: w, newSession: n }),
    onEvent: (m) => events.push(m),
    onAck: (id) => acks.push(id),
    onRejected: (id) => rejected.push(id),
  };
  const store = opts.store ?? new MemoryOutboxStore();
  const client = new ConnectionClient({
    url: 'ws://test/ws',
    hello: () => ({
      appVersion: '0.1.0',
      language: 'es-MX',
      accessibility: {
        font_scale: 1,
        high_contrast: false,
        reduced_motion: false,
        silent_mode: false,
      },
      entry: null,
    }),
    store,
    sessions: {
      load: () => saved,
      save: (s) => {
        saved = s;
      },
      clear: () => {
        saved = null;
      },
    },
    events: handlers,
    createSocket: () => new FakeSocket(),
    random: () => 0.5,
  });
  return { client, store, statuses, events, acks, rejected, welcomes, session: () => saved };
}

async function connected(t = setup()) {
  await t.client.start();
  latest().open();
  latest().receive(welcome());
  return t;
}

beforeEach(() => {
  FakeSocket.all = [];
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

describe('conexión y sesión', () => {
  it('saluda con la versión del protocolo y guarda la sesión', async () => {
    const t = await connected();
    const [hello] = latest().sentOf('hello');
    expect(hello).toMatchObject({ type: 'hello', protocol_version: 1, language: 'es-MX' });
    expect(hello).not.toHaveProperty('resume');
    expect(t.session()).toEqual({ session_id: 's-1', session_token: 'tok-1' });
    expect(t.statuses.at(-1)).toBe('online');
  });

  it('reanuda con la sesión guardada y el último seq', async () => {
    const t = setup({ session: { session_id: 's-1', session_token: 'tok-1' } });
    await t.client.start();
    latest().open();
    latest().receive(welcome({ resumed: true }));
    latest().receive(chat(1));
    latest().drop();
    await vi.advanceTimersByTimeAsync(20_000);
    latest().open();
    const [hello] = latest().sentOf('hello');
    expect(hello).toMatchObject({
      resume: { session_id: 's-1', session_token: 'tok-1', last_seq: 1 },
    });
  });

  it('avisa sesión nueva si el servidor no pudo reanudar', async () => {
    const t = setup({ session: { session_id: 'vieja', session_token: 'x' } });
    await t.client.start();
    latest().open();
    latest().receive(welcome({ resumed: false, session_id: 's-2' }));
    expect(t.welcomes[0]?.newSession).toBe(true);
    expect(t.session()?.session_id).toBe('s-2');
  });
});

describe('cola de pendientes: ningún mensaje se pierde', () => {
  it('lo escrito sin conexión se envía en orden al conectar', async () => {
    const t = setup();
    await t.client.start();
    const a = t.client.send({ type: 'chat_message', kind: 'text', text: 'uno' });
    const b = t.client.send({ type: 'chat_message', kind: 'text', text: 'dos' });
    latest().open();
    latest().receive(welcome());
    expect(
      latest()
        .sentOf('chat_message')
        .map((m) => m.client_msg_id),
    ).toEqual([a, b]);
  });

  it('un mensaje sin ack se reenvía con el MISMO id tras reconectar', async () => {
    const t = await connected();
    const id = t.client.send({ type: 'chat_message', kind: 'text', text: 'necesito ayuda' });
    expect(latest().sentOf('chat_message')).toHaveLength(1);
    latest().drop(); // se cae antes del ack
    await vi.advanceTimersByTimeAsync(20_000);
    latest().open();
    latest().receive(welcome({ resumed: true }));
    const resent = latest().sentOf('chat_message');
    expect(resent).toHaveLength(1);
    expect(resent[0]?.client_msg_id).toBe(id);
    latest().receive({ type: 'ack', client_msg_id: id, status: 'duplicate', server_ts: 'x' });
    expect(t.client.pendingCount).toBe(0);
  });

  it('la cola sobrevive a que se cierre la app (persistencia)', async () => {
    const store = new MemoryOutboxStore();
    const first = setup({ store });
    await first.client.start();
    const id = first.client.send({ type: 'chat_message', kind: 'text', text: 'ayuda' });
    await vi.advanceTimersByTimeAsync(0);
    first.client.stop();

    FakeSocket.all = [];
    const second = setup({ store });
    await second.client.start();
    latest().open();
    latest().receive(welcome());
    expect(latest().sentOf('chat_message')[0]?.client_msg_id).toBe(id);
  });

  it('acepta un id dado (acciones pedidas antes de que existiera el cliente)', async () => {
    const t = setup();
    const id = t.client.send({ type: 'video_call_request', media: 'video' }, 'id-temprano');
    expect(id).toBe('id-temprano');
    await t.client.start();
    latest().open();
    latest().receive(welcome());
    expect(latest().sentOf('video_call_request')[0]?.client_msg_id).toBe('id-temprano');
  });

  it('el ack saca el mensaje de la cola persistente', async () => {
    const t = await connected();
    const id = t.client.send({ type: 'create_incident', channel: 'text_chat' });
    latest().receive({ type: 'ack', client_msg_id: id, status: 'accepted', server_ts: 'x' });
    await vi.advanceTimersByTimeAsync(0);
    expect(await t.store.load()).toHaveLength(0);
    expect(t.acks).toEqual([id]);
  });

  it('error reintentable: se conserva y se reintenta; error definitivo: se reporta', async () => {
    const t = await connected();
    const retry = t.client.send({ type: 'chat_message', kind: 'text', text: 'a' });
    const invalid = t.client.send({ type: 'chat_message', kind: 'text', text: 'b' });
    latest().receive({
      type: 'error',
      code: 'internal',
      message: '',
      client_msg_id: retry,
      retryable: true,
      server_ts: 'x',
    });
    latest().receive({
      type: 'error',
      code: 'invalid_message',
      message: '',
      client_msg_id: invalid,
      retryable: false,
      server_ts: 'x',
    });
    expect(t.rejected).toEqual([invalid]);
    expect(t.client.isPending(retry)).toBe(true);
    const before = latest().sentOf('chat_message').length;
    await vi.advanceTimersByTimeAsync(2_500);
    expect(latest().sentOf('chat_message').length).toBe(before + 1);
  });
});

describe('orden y recuperación por seq', () => {
  it('entrega en orden aunque lleguen desordenados, sin duplicados, y confirma cada uno', async () => {
    const t = await connected();
    latest().receive(chat(2));
    expect(t.events).toHaveLength(0); // espera al 1
    latest().receive(chat(1));
    latest().receive(chat(1)); // duplicado
    latest().receive(chat(3));
    expect(t.events.map((e) => e.seq)).toEqual([1, 2, 3]);
    expect(
      latest()
        .sentOf('ack')
        .map((a) => (a as { seq: number }).seq),
    ).toEqual([1, 2, 1, 3]);
  });
});

describe('detección de caídas', () => {
  it('reconecta con backoff creciente', async () => {
    await connected();
    latest().drop();
    expect(FakeSocket.all).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(400);
    expect(FakeSocket.all).toHaveLength(2); // 1.er intento ~250 ms
    latest().drop();
    await vi.advanceTimersByTimeAsync(400);
    expect(FakeSocket.all).toHaveLength(2); // el 2.º espera más
    await vi.advanceTimersByTimeAsync(1_000);
    expect(FakeSocket.all).toHaveLength(3);
  });

  it('una conexión zombi (el servidor dejó de responder) se reemplaza', async () => {
    await connected();
    await vi.advanceTimersByTimeAsync(15_000);
    expect(latest().sentOf('ping')).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(31_000); // sin ninguna respuesta
    expect(FakeSocket.all.length).toBeGreaterThan(1);
  });

  it('al volver del segundo plano (iOS) reconecta de inmediato', async () => {
    const t = await connected();
    latest().readyState = 3; // iOS mató el socket sin avisar
    t.client.wake();
    expect(FakeSocket.all).toHaveLength(2);
  });

  it('sesión expirada (4410): sesión nueva inmediata y la cola se reenvía', async () => {
    const t = await connected();
    const id = t.client.send({ type: 'chat_message', kind: 'text', text: 'x' });
    latest().drop(4410);
    expect(t.session()).toBeNull();
    expect(FakeSocket.all).toHaveLength(2);
    latest().open();
    expect(latest().sentOf('hello')[0]).not.toHaveProperty('resume');
    latest().receive(welcome({ session_id: 's-9' }));
    expect(latest().sentOf('chat_message')[0]?.client_msg_id).toBe(id);
  });

  it('versión no soportada: deja de reconectar y avisa que hay que actualizar', async () => {
    const t = await connected();
    latest().drop(4426);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(FakeSocket.all).toHaveLength(1);
    expect(t.statuses.at(-1)).toBe('outdated');
  });
});

describe('backoff', () => {
  it('crece exponencialmente con jitter y respeta el máximo', () => {
    const max = new Backoff(500, 15_000, () => 0.999);
    const delays = Array.from({ length: 10 }, () => max.next());
    expect(delays[0]).toBeLessThan(500);
    expect(delays[5]).toBeGreaterThan(delays[1] ?? 0);
    expect(Math.max(...delays)).toBeLessThanOrEqual(15_000);
    expect(new Backoff(500, 15_000, () => 0).next()).toBe(100);
  });
});
