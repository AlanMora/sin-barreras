import { signal } from '@preact/signals';
import {
  Check,
  CheckCheck,
  Clock,
  Headset,
  MessageSquareText,
  SendHorizontal,
  TriangleAlert,
} from 'lucide-preact';
import { useEffect, useRef } from 'preact/hooks';
import { FALLBACK_PHONE, formatPhone } from '../config';
import { locale, type MessageKey, t } from '../i18n';
import { MAX_CHAT_TEXT_CHARS } from '../protocol/generated';
import { sendChat } from '../services';
import { type ChatItem, type LocalStatus, messages } from '../state/conversation';
import { SrOnly } from './ui';

/** Texto del cuadro de mensaje (compartido con las frases sugeridas). */
const draft = signal('');

function time(iso: string | undefined): string | null {
  if (!iso) return null;
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return null;
  return new Intl.DateTimeFormat(locale.value, { hour: '2-digit', minute: '2-digit' }).format(date);
}

function StatusMark({ status, at }: { status: LocalStatus; at: string | null }) {
  const label =
    status === 'failed'
      ? t('chat.status.failed', { phone: formatPhone(FALLBACK_PHONE) })
      : t(`chat.status.${status}`);
  const icon =
    status === 'queued' ? (
      <Clock size={15} />
    ) : status === 'sent' ? (
      <Check size={15} />
    ) : status === 'failed' ? (
      <TriangleAlert size={15} />
    ) : (
      <CheckCheck size={15} strokeWidth={status === 'read' ? 3 : 2} />
    );
  return (
    <span
      // Va sobre la burbuja propia (color primario): siempre `on-primary` para
      // conservar el contraste en las tres paletas.
      class={`mt-1.5 flex flex-wrap items-center justify-end gap-x-1.5 text-sm text-on-primary ${status === 'failed' ? 'font-extrabold underline' : ''}`}
    >
      {at ? <span>{at} ·</span> : null}
      <span aria-hidden="true">{icon}</span>
      <span>{label}</span>
    </span>
  );
}

function Bubble({ item }: { item: ChatItem }) {
  const mine = item.author === 'citizen';
  const at = time(item.serverTs);
  if (item.author === 'system') {
    return (
      <li class="sb-rise flex justify-center">
        <p class="rounded-full bg-surface-2 px-4 py-1.5 text-center text-sm font-bold text-fg">
          <SrOnly>{t('chat.system')}: </SrOnly>
          {item.text}
        </p>
      </li>
    );
  }
  if (mine) {
    return (
      <li class="sb-rise flex justify-end pl-10">
        <div class="max-w-full rounded-3xl rounded-br-md bg-primary px-4 py-3 text-on-primary shadow-card">
          <SrOnly>{t('chat.you')}: </SrOnly>
          <p class="whitespace-pre-wrap break-words text-lg leading-snug">{item.text}</p>
          {item.status ? <StatusMark status={item.status} at={at} /> : null}
        </div>
      </li>
    );
  }
  return (
    <li class="sb-rise flex items-end gap-2 pr-10">
      <span
        aria-hidden="true"
        class="grid h-9 w-9 shrink-0 place-items-center rounded-full sb-tint-1"
      >
        <Headset size={18} />
      </span>
      <div class="min-w-0 rounded-3xl rounded-bl-md border-2 border-line-soft bg-surface px-4 py-3 text-fg shadow-card">
        <SrOnly>{t('chat.operator')}: </SrOnly>
        <p class="whitespace-pre-wrap break-words text-lg leading-snug">{item.text}</p>
        {at ? <p class="mt-1 text-sm text-muted">{at}</p> : null}
      </div>
    </li>
  );
}

const SUGGESTIONS: MessageKey[] = [
  'chat.suggest.1',
  'chat.suggest.2',
  'chat.suggest.3',
  'chat.suggest.4',
];

function EmptyState({ suggestions }: { suggestions: boolean }) {
  return (
    <div class="flex flex-col items-center gap-4 rounded-card border-2 border-dashed border-line-soft px-4 py-6 text-center">
      <span aria-hidden="true" class="grid h-14 w-14 place-items-center rounded-2xl sb-tint-2">
        <MessageSquareText size={28} />
      </span>
      <p class="text-lg font-bold">{t('chat.empty')}</p>
      {suggestions ? (
        <div class="flex w-full flex-col gap-2">
          <p class="text-sm text-muted">{t('chat.suggest.title')}</p>
          <ul class="grid grid-cols-2 gap-2">
            {SUGGESTIONS.map((key) => (
              <li key={key}>
                <button
                  type="button"
                  onClick={() => {
                    draft.value = t(key);
                    document.getElementById('composer-text')?.focus();
                  }}
                  class="flex min-h-touch w-full items-center justify-center rounded-2xl border-2 border-line bg-surface px-3 py-2 font-bold leading-tight transition-colors hover:bg-surface-2"
                >
                  {t(key)}
                </button>
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  );
}

/**
 * Conversación. La lista NO es región viva (evita que el lector repita los
 * mensajes propios); los mensajes nuevos del C5 se anuncian por la región
 * única del Layout.
 */
export function MessageList({ suggestions = false }: { suggestions?: boolean }) {
  const list = messages.value;
  const endRef = useRef<HTMLLIElement>(null);
  useEffect(() => {
    endRef.current?.scrollIntoView({ block: 'end' });
  }, [list.length]);
  if (list.length === 0) return <EmptyState suggestions={suggestions} />;
  return (
    <ol aria-label={t('chat.log')} class="flex flex-col gap-3">
      {list.map((item) => (
        <Bubble key={item.key} item={item} />
      ))}
      <li ref={endRef} aria-hidden="true" />
    </ol>
  );
}

export function Composer() {
  const submit = (event: Event) => {
    event.preventDefault();
    if (!draft.value.trim()) return;
    sendChat(draft.value);
    draft.value = '';
  };
  return (
    <form
      onSubmit={submit}
      class="sb-safe-bottom sticky bottom-0 z-10 -mx-4 mt-auto flex items-end gap-2 border-t border-line-soft bg-bg/95 px-4 pt-3 backdrop-blur-md"
    >
      <label class="sb-field flex min-h-touch flex-1 rounded-3xl border-2 border-line bg-surface">
        <SrOnly>{t('chat.input')}</SrOnly>
        <textarea
          id="composer-text"
          value={draft.value}
          rows={1}
          maxLength={MAX_CHAT_TEXT_CHARS}
          placeholder={t('chat.placeholder')}
          onInput={(e) => {
            draft.value = e.currentTarget.value;
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey) submit(e);
          }}
          class="block max-h-40 min-h-[2.75rem] w-full resize-none bg-transparent px-4 py-2.5 text-lg text-fg placeholder:text-muted"
        />
      </label>
      <button
        type="submit"
        class="grid h-touch w-touch shrink-0 place-items-center rounded-full bg-accent text-on-accent shadow-card transition-transform hover:brightness-110 active:scale-95"
      >
        <SendHorizontal size={22} aria-hidden="true" />
        <SrOnly>{t('chat.send')}</SrOnly>
      </button>
    </form>
  );
}
