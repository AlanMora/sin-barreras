import { Check, CheckCheck, Clock, Send, TriangleAlert } from 'lucide-preact';
import { useEffect, useRef, useState } from 'preact/hooks';
import { FALLBACK_PHONE, formatPhone } from '../config';
import { t } from '../i18n';
import { MAX_CHAT_TEXT_CHARS } from '../protocol/generated';
import { sendChat } from '../services';
import { type ChatItem, type LocalStatus, messages } from '../state/conversation';
import { SrOnly } from './ui';

function StatusMark({ status }: { status: LocalStatus }) {
  const label =
    status === 'failed'
      ? t('chat.status.failed', { phone: formatPhone(FALLBACK_PHONE) })
      : t(`chat.status.${status}`);
  const icon =
    status === 'queued' ? (
      <Clock size={16} />
    ) : status === 'sent' ? (
      <Check size={16} />
    ) : status === 'failed' ? (
      <TriangleAlert size={16} />
    ) : (
      <CheckCheck size={16} strokeWidth={status === 'read' ? 3 : 2} />
    );
  return (
    <span
      // Va sobre la burbuja propia (color primario): siempre `on-primary` para
      // conservar el contraste en las tres paletas.
      class={`mt-1 flex items-center justify-end gap-1 text-sm text-on-primary ${status === 'failed' ? 'font-extrabold underline' : ''}`}
    >
      {icon}
      <span>{label}</span>
    </span>
  );
}

function Bubble({ item }: { item: ChatItem }) {
  const mine = item.author === 'citizen';
  const who = mine
    ? t('chat.you')
    : item.author === 'operator'
      ? t('chat.operator')
      : t('chat.system');
  return (
    <li class={`flex ${mine ? 'justify-end' : 'justify-start'}`}>
      <div
        class={`max-w-[85%] rounded-card px-4 py-3 ${
          mine
            ? 'rounded-br-md bg-primary text-on-primary'
            : 'rounded-bl-md border-2 border-line bg-surface text-fg'
        }`}
      >
        <SrOnly>{who}: </SrOnly>
        <p class="whitespace-pre-wrap break-words text-lg">{item.text}</p>
        {mine && item.status ? <StatusMark status={item.status} /> : null}
      </div>
    </li>
  );
}

/**
 * Conversación. La lista NO es región viva (evita que el lector repita los
 * mensajes propios); los mensajes nuevos del C5 se anuncian por la región
 * única del Layout.
 */
export function MessageList() {
  const list = messages.value;
  const endRef = useRef<HTMLLIElement>(null);
  useEffect(() => {
    endRef.current?.scrollIntoView({ block: 'end' });
  }, [list.length]);
  if (list.length === 0) {
    return <p class="text-lg text-muted">{t('chat.empty')}</p>;
  }
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
  const [text, setText] = useState('');
  const submit = (event: Event) => {
    event.preventDefault();
    if (!text.trim()) return;
    sendChat(text);
    setText('');
  };
  return (
    <form onSubmit={submit} class="sticky bottom-0 flex items-end gap-2 bg-bg pb-2 pt-2">
      <label class="flex-1">
        <span class="mb-1 block font-bold">{t('chat.input')}</span>
        <textarea
          value={text}
          rows={2}
          maxLength={MAX_CHAT_TEXT_CHARS}
          onInput={(e) => setText(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey) submit(e);
          }}
          class="block min-h-touch w-full resize-none rounded-button border-2 border-line bg-bg px-3 py-2 text-lg text-fg"
        />
      </label>
      <button
        type="submit"
        class="inline-flex min-h-touch min-w-touch items-center justify-center gap-2 rounded-button bg-accent px-4 text-lg font-extrabold text-on-accent"
      >
        <Send size={22} />
        <span>{t('chat.send')}</span>
      </button>
    </form>
  );
}
