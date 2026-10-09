import {
  ChevronRight,
  HandHelping,
  Languages,
  MessageSquareText,
  MicOff,
  ShieldCheck,
  Video,
  Volume2,
} from 'lucide-preact';
import type { ComponentChildren } from 'preact';
import { useEffect } from 'preact/hooks';
import { FallbackContact } from '../components/FallbackContact';
import { IncidentStatus } from '../components/IncidentStatus';
import { PageTitle } from '../components/ui';
import { type MessageKey, t } from '../i18n';
import type { Channel } from '../protocol/generated';
import { clientEvent } from '../services';
import { connectionStatus, helpRequested } from '../state/conversation';

interface Option {
  href: string;
  key: Channel;
  icon: ComponentChildren;
}

const OPTIONS: Option[] = [
  { href: '/llamada?m=video', key: 'lsm_video', icon: <Video size={26} /> },
  { href: '/chat', key: 'text_chat', icon: <MessageSquareText size={26} /> },
  { href: '/llamada?m=audio', key: 'audio', icon: <Volume2 size={26} /> },
  { href: '/idioma', key: 'translation', icon: <Languages size={26} /> },
  { href: '/silencioso', key: 'silent', icon: <MicOff size={26} /> },
];

/** Pantalla de la conversación activa, para volver a ella desde el inicio. */
const CONVERSATION: Record<Channel, string> = {
  lsm_video: '/llamada?m=video',
  audio: '/llamada?m=audio',
  text_chat: '/chat',
  translation: '/chat',
  silent: '/silencioso',
};

/** Indicador breve de la conexión (no es región viva: los cambios ya los
 *  anuncia el aviso de conexión). */
function StatusChip() {
  const status = connectionStatus.value;
  const online = status === 'online';
  const offline = status === 'offline' || status === 'reconnecting';
  const label = online
    ? t('home.status.online')
    : offline
      ? t('home.status.offline')
      : t('home.status.connecting');
  return (
    <p class="inline-flex items-center gap-2 rounded-full border border-glass bg-surface px-3 py-1.5 text-sm font-bold">
      <span
        aria-hidden="true"
        class={`block h-2.5 w-2.5 rounded-full ${online ? 'bg-success' : offline ? 'bg-danger' : 'sb-pulse bg-accent'}`}
      />
      {label}
    </p>
  );
}

export function Home() {
  useEffect(() => {
    // El aviso está visible en esta pantalla desde el primer momento.
    clientEvent('privacy_notice_shown');
    clientEvent('channel_screen_shown');
  }, []);
  const active = helpRequested.value;
  return (
    <>
      <div class="flex flex-col items-center gap-3 pt-1 text-center">
        <PageTitle documentTitle={`${t('app.name')} · ${t('home.title')}`}>
          {t('home.title')}
        </PageTitle>
        <p class="max-w-md text-base text-muted sm:text-lg">{t('home.subtitle')}</p>
        <StatusChip />
      </div>

      {active ? (
        <div class="flex flex-col gap-3">
          <IncidentStatus />
          <a href={CONVERSATION[active]} class="sb-cta">
            {t('home.cta_back')}
            <ChevronRight size={28} strokeWidth={3} aria-hidden="true" />
          </a>
        </div>
      ) : null}

      <section aria-labelledby="channels-title">
        <h2 id="channels-title" class="sr-only">
          {t('home.channels')}
        </h2>
        {/* 2 columnas en pantallas normales; 1 sola con letra grande. */}
        <ul class="grid grid-cols-[repeat(auto-fit,minmax(9.5rem,1fr))] gap-3">
          {OPTIONS.map((option) => (
            <li
              key={option.key}
              class={`sb-rise ${option.key === 'silent' ? 'col-span-full' : ''}`}
            >
              <a
                href={option.href}
                class="sb-glass group flex h-full min-h-[8rem] flex-col items-center justify-center gap-2 px-3 py-4 text-center transition-[transform,border-color] hover:-translate-y-0.5 hover:border-line active:translate-y-0"
              >
                <span
                  aria-hidden="true"
                  class="grid h-12 w-12 place-items-center rounded-2xl sb-tint-2 transition-transform group-hover:scale-105"
                >
                  {option.icon}
                </span>
                <span>
                  <span class="block font-display text-[1.05rem] font-extrabold leading-tight">
                    {t(`channel.${option.key}.title` as MessageKey)}
                  </span>
                  <span class="mt-1 block text-sm leading-snug text-muted">
                    {t(`channel.${option.key}.desc` as MessageKey)}
                  </span>
                </span>
              </a>
            </li>
          ))}
        </ul>
      </section>

      {active ? null : (
        <div class="flex flex-col items-center gap-2">
          <p class="text-center text-muted">{t('home.cta_hint')}</p>
          <a href="/chat" class="sb-cta">
            {t('home.cta')}
            <ChevronRight size={28} strokeWidth={3} aria-hidden="true" />
          </a>
        </div>
      )}

      {/* Aviso de privacidad: visible, breve y NUNCA bloquea pedir ayuda. */}
      <section aria-labelledby="privacy-short" class="sb-card flex items-start gap-3 p-4">
        <span
          aria-hidden="true"
          class="grid h-10 w-10 shrink-0 place-items-center rounded-xl sb-tint-3"
        >
          <ShieldCheck size={20} />
        </span>
        <div class="min-w-0">
          <p id="privacy-short" class="text-base">
            {t('privacy.short')}
          </p>
          <a
            href="/aviso-de-privacidad"
            class="inline-flex min-h-touch items-center font-bold text-accent-text underline decoration-2 underline-offset-4"
          >
            {t('privacy.link')}
          </a>
        </div>
      </section>

      <section aria-labelledby="fallback-title" class="sb-card p-4">
        <h2
          id="fallback-title"
          class="mb-1 flex items-center gap-2 font-display text-lg font-extrabold"
        >
          <HandHelping size={22} aria-hidden="true" class="text-accent-text" />
          {t('fallback.title')}
        </h2>
        <FallbackContact compact />
      </section>

      <p class="text-center text-sm text-muted">{t('app.program')}</p>
    </>
  );
}
