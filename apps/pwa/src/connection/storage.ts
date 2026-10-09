/**
 * Persistencia local mínima: cola de pendientes en IndexedDB (sobrevive a que
 * iOS mate la pestaña) y sesión en localStorage. Si el navegador no permite
 * almacenamiento (modo privado estricto), se usa memoria: la app sigue
 * funcionando, solo pierde la cola si se recarga.
 */
import type { ClientMessage } from '../protocol/generated';

export interface QueuedCommand {
  /** Orden de envío (los pendientes se reenvían en orden). */
  order: number;
  message: ClientMessage;
}

export interface OutboxStore {
  load(): Promise<QueuedCommand[]>;
  put(command: QueuedCommand): Promise<void>;
  remove(clientMsgId: string): Promise<void>;
}

export class MemoryOutboxStore implements OutboxStore {
  private readonly items = new Map<string, QueuedCommand>();

  async load(): Promise<QueuedCommand[]> {
    return [...this.items.values()].sort((a, b) => a.order - b.order);
  }

  async put(command: QueuedCommand): Promise<void> {
    this.items.set(command.message.client_msg_id, command);
  }

  async remove(clientMsgId: string): Promise<void> {
    this.items.delete(clientMsgId);
  }
}

const DB_NAME = 'sin-barreras';
const STORE = 'outbox';

export class IndexedDbOutboxStore implements OutboxStore {
  private db: Promise<IDBDatabase>;

  constructor(factory: IDBFactory = indexedDB) {
    this.db = new Promise((resolve, reject) => {
      const request = factory.open(DB_NAME, 1);
      request.onupgradeneeded = () => {
        request.result.createObjectStore(STORE);
      };
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
  }

  private async run<T>(
    mode: IDBTransactionMode,
    op: (s: IDBObjectStore) => IDBRequest<T>,
  ): Promise<T> {
    const db = await this.db;
    return new Promise((resolve, reject) => {
      const tx = db.transaction(STORE, mode);
      const request = op(tx.objectStore(STORE));
      tx.oncomplete = () => resolve(request.result);
      tx.onerror = () => reject(tx.error);
      tx.onabort = () => reject(tx.error);
    });
  }

  async load(): Promise<QueuedCommand[]> {
    const all = (await this.run('readonly', (s) => s.getAll())) as QueuedCommand[];
    return all.sort((a, b) => a.order - b.order);
  }

  async put(command: QueuedCommand): Promise<void> {
    await this.run('readwrite', (s) => s.put(command, command.message.client_msg_id));
  }

  async remove(clientMsgId: string): Promise<void> {
    await this.run('readwrite', (s) => s.delete(clientMsgId));
  }
}

/** Elige IndexedDB si funciona; si no, memoria. */
export async function createOutboxStore(): Promise<OutboxStore> {
  try {
    if (typeof indexedDB === 'undefined') return new MemoryOutboxStore();
    const store = new IndexedDbOutboxStore();
    await store.load();
    return store;
  } catch {
    return new MemoryOutboxStore();
  }
}

export interface StoredSession {
  session_id: string;
  session_token: string;
}

const SESSION_KEY = 'sb.session';

export const sessionStore = {
  load(): StoredSession | null {
    try {
      const raw = localStorage.getItem(SESSION_KEY);
      return raw ? (JSON.parse(raw) as StoredSession) : null;
    } catch {
      return null;
    }
  },
  save(session: StoredSession): void {
    try {
      localStorage.setItem(SESSION_KEY, JSON.stringify(session));
    } catch {
      // Sin almacenamiento: la sesión vive solo en memoria.
    }
  },
  clear(): void {
    try {
      localStorage.removeItem(SESSION_KEY);
    } catch {
      // nada que limpiar
    }
  },
};
