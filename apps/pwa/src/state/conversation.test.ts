import { beforeEach, describe, expect, it } from 'vitest';
import type { SequencedMessage, Welcome } from '../protocol/generated';
import {
  addLocalMessage,
  announcement,
  applyEvent,
  applyWelcome,
  incident,
  markAcked,
  messages,
  resetConversation,
} from './conversation';

const texts = {
  newOperatorMessage: (t: string) => `nuevo: ${t}`,
  incidentStatus: (s: string) => `estado: ${s}`,
};
const ev = (m: object) => m as SequencedMessage;

beforeEach(resetConversation);

describe('conversación', () => {
  it('estados de un mensaje: en cola → enviado → entregado → leído (nunca retroceden)', () => {
    addLocalMessage({
      key: 'k',
      author: 'citizen',
      kind: 'text',
      text: 'ayuda',
      clientMsgId: 'c1',
      status: 'queued',
    });
    markAcked('c1');
    expect(messages.value[0]?.status).toBe('sent');
    applyEvent(
      ev({ seq: 1, type: 'message_status', client_msg_id: 'c1', status: 'read', server_ts: '' }),
      texts,
    );
    applyEvent(
      ev({
        seq: 2,
        type: 'message_status',
        client_msg_id: 'c1',
        status: 'delivered',
        server_ts: '',
      }),
      texts,
    );
    expect(messages.value[0]?.status).toBe('read');
  });

  it('el eco del servidor no duplica el mensaje local', () => {
    addLocalMessage({
      key: 'k',
      author: 'citizen',
      kind: 'text',
      text: 'hola',
      clientMsgId: 'c1',
      status: 'queued',
    });
    applyEvent(
      ev({
        seq: 1,
        type: 'chat_message',
        message_id: 'm1',
        author: 'citizen',
        kind: 'text',
        text: 'hola',
        client_msg_id: 'c1',
        server_ts: 't',
      }),
      texts,
    );
    expect(messages.value).toHaveLength(1);
    expect(messages.value[0]?.status).toBe('sent');
  });

  it('tras recargar, la conversación se reconstruye desde el servidor sin duplicados', () => {
    const op = ev({
      seq: 2,
      type: 'chat_message',
      message_id: 'm2',
      author: 'operator',
      kind: 'text',
      text: 'hola',
      server_ts: 't',
    });
    applyEvent(op, texts);
    applyEvent(op, texts);
    expect(messages.value).toHaveLength(1);
    expect(announcement.value?.text).toBe('nuevo: hola');
  });

  it('estado del incidente con tiempo estimado', () => {
    applyEvent(
      ev({
        seq: 1,
        type: 'incident_created',
        incident_id: 'i',
        channel: 'silent',
        client_msg_id: 'c',
        server_ts: '',
      }),
      texts,
    );
    applyEvent(
      ev({
        seq: 2,
        type: 'incident_status',
        status: 'unit_en_route',
        eta_seconds: 480,
        server_ts: '',
      }),
      texts,
    );
    expect(incident.value).toMatchObject({
      channel: 'silent',
      status: 'unit_en_route',
      etaSeconds: 480,
    });
    expect(announcement.value?.text).toBe('estado: unit_en_route');
  });

  it('sesión nueva: se descarta lo confirmado pero se conservan los pendientes', () => {
    addLocalMessage({
      key: 'a',
      author: 'citizen',
      kind: 'text',
      text: 'ya enviado',
      clientMsgId: 'a',
      status: 'sent',
    });
    addLocalMessage({
      key: 'b',
      author: 'citizen',
      kind: 'text',
      text: 'pendiente',
      clientMsgId: 'b',
      status: 'queued',
    });
    applyWelcome({ entry: 'none' } as Welcome, true, (id) => id === 'b');
    expect(messages.value.map((m) => m.text)).toEqual(['pendiente']);
  });
});
