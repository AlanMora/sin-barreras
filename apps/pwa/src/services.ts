/**
 * Arranque de la conexión y acciones de la interfaz.
 *
 * La conexión se abre en cuanto se abre la app (antes de que el ciudadano
 * elija canal): cuando toque "pedir ayuda", la sesión ya está lista.
 */
import {
  type ConnectionClient as Client,
  ConnectionClient,
  type OutgoingCommand,
} from './connection/client';
import { transportFactory } from './connection/http-socket';
import { createOutboxStore, sessionStore } from './connection/storage';
import { locale, t } from './i18n';
import { captureEntry } from './lib/entry';
import { nowIso, uuid } from './lib/ids';
import { startLocationSharing } from './location/location';
import type {
  CallMedia,
  Channel,
  ClientEventKind,
  IncidentState,
  QuickReply,
} from './protocol/generated';
import {
  addLocalMessage,
  applyEvent,
  applyWelcome,
  call,
  connectionStatus,
  helpRequested,
  markAcked,
  markFailed,
} from './state/conversation';
import { accessibilitySnapshot } from './state/settings';

declare const __APP_VERSION__: string;

let client: Client | null = null;
/**
 * Acciones pedidas antes de que exista el cliente (p. ej. al abrir directo
 * `/llamada`, la pantalla pide la llamada mientras IndexedDB aún abre). Se
 * guardan con su id definitivo y se entregan al cliente en cuanto existe:
 * ninguna acción se pierde por llegar "demasiado pronto".
 */
const early: { command: OutgoingCommand; id: string }[] = [];

function wsUrl(): string {
  const configured = import.meta.env.VITE_WS_URL;
  if (configured) return configured;
  return `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws`;
}

function statusText(status: IncidentState, eta?: number): string {
  if (status === 'unit_en_route' && eta) {
    return t('incident.unit_en_route_eta', { minutes: Math.max(1, Math.round(eta / 60)) });
  }
  return t(`incident.${status}`);
}

export async function startServices(): Promise<void> {
  const entry = captureEntry();
  const store = await createOutboxStore();
  client = new ConnectionClient({
    url: wsUrl(),
    // WebSocket mientras funcione; si la red lo bloquea, HTTP (ADR-0018).
    createSocket: transportFactory(wsUrl(), import.meta.env.VITE_API_URL ?? ''),
    hello: () => ({
      appVersion: __APP_VERSION__,
      language: locale.value,
      accessibility: accessibilitySnapshot(),
      entry,
    }),
    store,
    sessions: sessionStore,
    isOnline: () => navigator.onLine,
    events: {
      onStatus: (status) => {
        connectionStatus.value = status;
      },
      onWelcome: (welcome, newSession) => {
        applyWelcome(welcome, newSession, (id) => client?.isPending(id) ?? false);
      },
      onEvent: (message) =>
        applyEvent(message, {
          newOperatorMessage: (text) => t('chat.new_operator_message', { text }),
          incidentStatus: statusText,
        }),
      onAck: (id) => markAcked(id),
      onRejected: (id) => markFailed(id),
    },
  });

  // iOS/Safari suspende el WebSocket al bloquear la pantalla o cambiar de app:
  // al volver, se reconecta y se recupera lo pendiente por `seq`.
  const wake = () => client?.wake();
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible') wake();
  });
  window.addEventListener('pageshow', wake);
  window.addEventListener('focus', wake);
  window.addEventListener('online', wake);
  window.addEventListener('offline', () => client?.wentOffline());

  await client.load();
  // Mensajes que quedaron en cola (p. ej. se recargó sin conexión): se vuelven
  // a mostrar como "en cola" para que nadie crea que se perdieron.
  for (const message of client.pending()) {
    if (message.type === 'chat_message') {
      helpRequested.value ??= message.kind === 'quick_reply' ? 'silent' : 'text_chat';
      addLocalMessage({
        key: message.client_msg_id,
        author: 'citizen',
        kind: message.kind,
        text: message.text,
        ...(message.quick_reply ? { quickReply: message.quick_reply } : {}),
        clientMsgId: message.client_msg_id,
        status: 'queued',
      });
    }
  }
  for (const { command, id } of early.splice(0)) client.send(command, id);
  client.connectNow();
  clientEvent('app_opened', entry ? `qr:${entry.point_id}` : 'web');
}

function send(command: OutgoingCommand): string {
  if (client) return client.send(command);
  const id = uuid();
  early.push({ command, id });
  return id;
}

export function clientEvent(kind: ClientEventKind, detail?: string): void {
  send({ type: 'client_event', kind, ...(detail ? { detail } : {}) });
}

function shareLocation(): void {
  startLocationSharing({
    send: (fix) =>
      send({
        type: 'location_update',
        latitude: fix.latitude,
        longitude: fix.longitude,
        accuracy_m: fix.accuracy,
        source: 'gps',
        captured_at: new Date(fix.timestamp).toISOString(),
      }),
    granted: () => clientEvent('permission_granted', 'geolocation'),
    denied: () => clientEvent('permission_denied', 'geolocation'),
  });
}

/** Pide ayuda por un canal (idempotente: solo la primera vez crea el incidente). */
export function requestHelp(channel: Channel, language?: string): void {
  if (!helpRequested.value) {
    helpRequested.value = channel;
    send({ type: 'create_incident', channel, ...(language ? { language } : {}) });
  }
  // El permiso de ubicación se pide justo ahora: cuando se necesita.
  shareLocation();
}

export function sendChat(text: string): void {
  const trimmed = text.trim();
  if (!trimmed) return;
  helpRequested.value ??= 'text_chat';
  const id = send({ type: 'chat_message', kind: 'text', text: trimmed });
  if (id)
    addLocalMessage({
      key: id,
      author: 'citizen',
      kind: 'text',
      text: trimmed,
      clientMsgId: id,
      status: 'queued',
    });
}

export function sendQuickReply(code: QuickReply): void {
  const text = t(`silent.quick.${code}`);
  helpRequested.value ??= 'silent';
  const id = send({ type: 'chat_message', kind: 'quick_reply', text, quick_reply: code });
  if (id) {
    addLocalMessage({
      key: id,
      author: 'citizen',
      kind: 'quick_reply',
      text,
      quickReply: code,
      clientMsgId: id,
      status: 'queued',
    });
  }
}

export function requestCall(media: CallMedia): void {
  if (call.value && call.value.status !== 'failed' && call.value.status !== 'ended') return;
  const id = send({ type: 'video_call_request', media });
  call.value = { requestId: id, media, status: 'pending' };
}

/** Salida rápida del modo silencioso: la sesión sigue viva en segundo plano. */
export function quickExit(): void {
  clientEvent('quick_exit_used', nowIso());
}
