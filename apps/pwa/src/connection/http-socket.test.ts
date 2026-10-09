import { describe, expect, it, vi } from 'vitest';
import { HttpSocket } from './http-socket';

type Handler = (
  body: Record<string, unknown>,
  signal?: AbortSignal,
) => Promise<{ status: number; json: unknown }>;

function fakeFetch(handlers: Record<string, Handler>) {
  const calls: { path: string; body: Record<string, unknown> }[] = [];
  const impl = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const path = String(input).replace('http://x', '');
    const body = JSON.parse(String(init?.body)) as Record<string, unknown>;
    calls.push({ path, body });
    const handler = handlers[path];
    if (!handler) throw new Error(`sin handler ${path}`);
    const { status, json } = await handler(body, init?.signal ?? undefined);
    return { ok: status < 400, status, json: async () => json } as Response;
  }) as typeof fetch;
  return { impl, calls };
}

const welcome = { type: 'welcome', session_id: 's', session_token: 't', resumed: false };
const tick = () => new Promise((r) => setTimeout(r, 0));

describe('transporte HTTP de respaldo', () => {
  it('traduce hello y sync a mensajes como un WebSocket', async () => {
    let syncs = 0;
    const { impl, calls } = fakeFetch({
      '/api/v1/hello': async () => ({
        status: 200,
        json: { welcome, frames: [{ seq: 1, type: 'incident_status' }] },
      }),
      '/api/v1/sync': async () => {
        syncs += 1;
        if (syncs === 1) {
          return {
            status: 200,
            json: {
              replies: [{ type: 'ack', client_msg_id: 'c1', status: 'accepted' }],
              frames: [{ seq: 2 }],
              latest_seq: 2,
            },
          };
        }
        return new Promise(() => undefined); // long-poll que no termina
      },
    });
    const socket = new HttpSocket('http://x', impl);
    const received: unknown[] = [];
    socket.onmessage = (ev) => received.push(JSON.parse(String(ev.data)));
    await tick();
    expect(socket.readyState).toBe(1);
    socket.send(JSON.stringify({ type: 'hello', client_msg_id: 'h' }));
    socket.send(
      JSON.stringify({ type: 'chat_message', client_msg_id: 'c1', kind: 'text', text: 'hola' }),
    );
    await vi.waitFor(() => expect(received).toHaveLength(4));
    expect(
      received.map(
        (r) => (r as { type?: string; seq?: number }).type ?? (r as { seq: number }).seq,
      ),
    ).toEqual(['welcome', 'incident_status', 'ack', 2]);
    const sync = calls.find((c) => c.path === '/api/v1/sync');
    expect(sync?.body).toMatchObject({ session_id: 's', session_token: 't', last_seq: 1 });
    expect(sync?.body.commands).toHaveLength(1);
    socket.close();
  });

  it('responde los pings localmente (el ciclo HTTP es el latido)', async () => {
    const socket = new HttpSocket('http://x', fakeFetch({}).impl);
    const received: string[] = [];
    socket.onmessage = (ev) => received.push(String(ev.data));
    await tick();
    socket.send(JSON.stringify({ type: 'ping', client_msg_id: 'p' }));
    expect(JSON.parse(received[0] ?? '{}')).toMatchObject({ type: 'pong', client_msg_id: 'p' });
    socket.close();
  });

  it('sesión inválida → cierre 4410 (el cliente hace hello de nuevo)', async () => {
    const { impl } = fakeFetch({
      '/api/v1/hello': async () => ({ status: 200, json: { welcome, frames: [] } }),
      '/api/v1/sync': async () => ({
        status: 401,
        json: { type: 'error', code: 'hello_required' },
      }),
    });
    const socket = new HttpSocket('http://x', impl);
    const codes: number[] = [];
    socket.onclose = (ev) => codes.push(ev.code);
    await tick();
    socket.send(JSON.stringify({ type: 'hello', client_msg_id: 'h' }));
    await vi.waitFor(() => expect(codes).toEqual([4410]));
  });

  it('una falla de red cierra con 1006 para que el cliente reconecte', async () => {
    const socket = new HttpSocket('http://x', (async () => {
      throw new TypeError('network');
    }) as typeof fetch);
    const codes: number[] = [];
    socket.onclose = (ev) => codes.push(ev.code);
    await tick();
    socket.send(JSON.stringify({ type: 'hello', client_msg_id: 'h' }));
    await vi.waitFor(() => expect(codes).toEqual([1006]));
  });
});
