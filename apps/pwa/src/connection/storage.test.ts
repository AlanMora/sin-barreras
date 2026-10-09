import 'fake-indexeddb/auto';
import { describe, expect, it } from 'vitest';
import type { ClientMessage } from '../protocol/generated';
import { IndexedDbOutboxStore } from './storage';

const msg = (id: string): ClientMessage => ({
  type: 'chat_message',
  client_msg_id: id,
  kind: 'text',
  text: id,
});

describe('cola en IndexedDB', () => {
  it('guarda, ordena y borra', async () => {
    const store = new IndexedDbOutboxStore();
    await store.put({ order: 2, message: msg('b') });
    await store.put({ order: 1, message: msg('a') });
    expect((await store.load()).map((q) => q.message.client_msg_id)).toEqual(['a', 'b']);
    await store.remove('a');
    expect((await store.load()).map((q) => q.message.client_msg_id)).toEqual(['b']);
  });

  it('sobrevive a reabrir la base (como al recargar la página)', async () => {
    await new IndexedDbOutboxStore().put({ order: 9, message: msg('persistente') });
    const reopened = await new IndexedDbOutboxStore().load();
    expect(reopened.some((q) => q.message.client_msg_id === 'persistente')).toBe(true);
  });
});
