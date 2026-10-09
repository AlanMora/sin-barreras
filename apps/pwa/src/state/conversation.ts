/**
 * Estado de la conversación, del incidente y de la llamada, alimentado por los
 * eventos del servidor. Las funciones son puras respecto del DOM para poder
 * probarlas.
 */
import { signal } from '@preact/signals';
import type { ConnectionStatus } from '../connection/client';
import type {
  Channel,
  ChatKind,
  DeliveryStatus,
  EntryVerification,
  IncidentState,
  QuickReply,
  SequencedMessage,
  VideoCallState,
  Welcome,
} from '../protocol/generated';

export type LocalStatus = 'queued' | 'sent' | DeliveryStatus;

export interface ChatItem {
  /** Llave estable para la lista. */
  key: string;
  author: 'citizen' | 'operator' | 'system';
  kind: ChatKind;
  text: string;
  quickReply?: QuickReply;
  clientMsgId?: string;
  status?: LocalStatus;
  serverTs?: string;
}

export interface IncidentInfo {
  id: string | null;
  channel: Channel;
  status: IncidentState;
  etaSeconds?: number;
}

export interface CallInfo {
  requestId: string;
  media: 'video' | 'audio';
  status: VideoCallState | 'pending';
  join?: Record<string, unknown>;
}

export const connectionStatus = signal<ConnectionStatus>('connecting');
export const entryVerification = signal<EntryVerification>('none');
export const incident = signal<IncidentInfo | null>(null);
/** El ciudadano ya pidió ayuda (aunque el C5 no haya confirmado aún). */
export const helpRequested = signal<Channel | null>(null);
export const messages = signal<ChatItem[]>([]);
export const call = signal<CallInfo | null>(null);
/** Último aviso para la región `aria-live` (lectores de pantalla). */
export const announcement = signal<{ text: string; id: number } | null>(null);

let announceId = 0;
export function announce(text: string): void {
  announceId += 1;
  announcement.value = { text, id: announceId };
}

const RANK: Record<LocalStatus, number> = { queued: 0, sent: 1, delivered: 2, read: 3, failed: 4 };

function upgrade(current: LocalStatus | undefined, next: LocalStatus): LocalStatus {
  if (!current) return next;
  return RANK[next] > RANK[current] ? next : current;
}

function updateByClientId(clientMsgId: string, update: (item: ChatItem) => ChatItem): boolean {
  let found = false;
  messages.value = messages.value.map((item) => {
    if (item.clientMsgId !== clientMsgId) return item;
    found = true;
    return update(item);
  });
  return found;
}

export function addLocalMessage(item: ChatItem): void {
  messages.value = [...messages.value, item];
}

export function markAcked(clientMsgId: string): void {
  updateByClientId(clientMsgId, (item) => ({ ...item, status: upgrade(item.status, 'sent') }));
}

export function markFailed(clientMsgId: string): void {
  updateByClientId(clientMsgId, (item) => ({ ...item, status: 'failed' }));
  if (call.value?.requestId === clientMsgId) call.value = { ...call.value, status: 'failed' };
}

/** Bienvenida del servidor. En una sesión nueva la conversación empieza de cero,
 *  pero se conservan los mensajes locales aún pendientes (se reenviarán). */
export function applyWelcome(
  welcome: Welcome,
  newSession: boolean,
  isPending: (id: string) => boolean,
): void {
  entryVerification.value = welcome.entry;
  if (newSession) {
    messages.value = messages.value.filter((m) => m.clientMsgId && isPending(m.clientMsgId));
    incident.value = null;
  }
  if (welcome.incident) {
    incident.value = {
      id: welcome.incident.incident_id,
      channel: welcome.incident.channel,
      status: welcome.incident.status,
    };
    helpRequested.value = welcome.incident.channel;
  }
}

export interface EventTexts {
  newOperatorMessage(text: string): string;
  incidentStatus(status: IncidentState, etaSeconds?: number): string;
}

/** Aplica un mensaje secuenciado del servidor. */
export function applyEvent(message: SequencedMessage, texts: EventTexts): void {
  switch (message.type) {
    case 'incident_created':
      incident.value = { id: message.incident_id, channel: message.channel, status: 'received' };
      helpRequested.value ??= message.channel;
      break;
    case 'incident_status': {
      const base = incident.value ?? {
        id: null,
        channel: helpRequested.value ?? 'text_chat',
        status: 'received',
      };
      incident.value = {
        ...base,
        status: message.status,
        ...(message.eta_seconds !== undefined ? { etaSeconds: message.eta_seconds } : {}),
      };
      announce(texts.incidentStatus(message.status, message.eta_seconds));
      break;
    }
    case 'chat_message': {
      if (message.author === 'citizen' && message.client_msg_id) {
        const known = updateByClientId(message.client_msg_id, (item) => ({
          ...item,
          serverTs: message.server_ts,
          status: upgrade(item.status, 'sent'),
        }));
        if (known) break;
      }
      if (messages.value.some((m) => m.key === message.message_id)) break;
      messages.value = [
        ...messages.value,
        {
          key: message.message_id,
          author: message.author,
          kind: message.kind,
          text: message.text,
          ...(message.quick_reply ? { quickReply: message.quick_reply } : {}),
          ...(message.client_msg_id
            ? { clientMsgId: message.client_msg_id, status: 'sent' as LocalStatus }
            : {}),
          serverTs: message.server_ts,
        },
      ];
      if (message.author === 'operator') announce(texts.newOperatorMessage(message.text));
      break;
    }
    case 'message_status':
      updateByClientId(message.client_msg_id, (item) => ({
        ...item,
        status: upgrade(item.status, message.status),
      }));
      break;
    case 'video_call': {
      const current = call.value;
      call.value = {
        requestId: message.request_client_msg_id,
        media: current?.requestId === message.request_client_msg_id ? current.media : 'video',
        status: message.status,
        ...(message.join ? { join: message.join } : current?.join ? { join: current.join } : {}),
      };
      break;
    }
    case 'session_closed':
      if (incident.value) incident.value = { ...incident.value, status: 'closed' };
      break;
  }
}

/** Reinicia el estado (pruebas). */
export function resetConversation(): void {
  connectionStatus.value = 'connecting';
  entryVerification.value = 'none';
  incident.value = null;
  helpRequested.value = null;
  messages.value = [];
  call.value = null;
  announcement.value = null;
}
