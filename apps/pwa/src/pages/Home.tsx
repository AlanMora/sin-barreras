import {
  ChevronRight,
  HandHelping,
  Languages,
  MessageSquareText,
  Phone,
  ShieldCheck,
  Video,
  VolumeX,
} from 'lucide-preact';
import type { ComponentChildren } from 'preact';
import { useEffect } from 'preact/hooks';
import { FallbackContact } from '../components/FallbackContact';
import { IncidentStatus } from '../components/IncidentStatus';
import { PageTitle } from '../components/ui';
import { type MessageKey, t } from '../i18n';
import { clientEvent } from '../services';
import { connectionStatus, helpRequested } from '../state/conversation';

interface Option {
  href: string;
  key: 'lsm_video' | 'text_chat' | 'audio' | 'translation' | 'silent';
  icon: ComponentChildren;
  /** Mosaico de color del canal (`.sb-tint-N` en styles.css). */
  tint: string;
}

const OPTIONS: Option[] = [
  { href: '/llamada?m=video', key: 'lsm_video', icon: <Video size={28} />, tint: 'sb-tint-1' },
  { href: '/chat', key: 'text_chat', icon: <MessageSquareText size={28} />, tint: 'sb-tint-2' },
  { href: '/llamada?m=audio', key: 'audio', icon: <Phone size={28} />, tint: 'sb-tint-3' },
  { href: '/idioma', key: 'translation', icon: <Languages size={28} />, tint: 'sb-tint-4' },
  { href: '/silencioso', key: 'silent', icon: <VolumeX size={28} />, tint: 'sb-tint-5' },
];

/** Indicador breve de la conexión dentro del encabezado (no es región viva:
 *  los cambios ya los anuncia el aviso de conexión). */
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
    <p class="inline-flex items-center gap-2 self-start rounded-full bg-white/10 px-3 py-1.5 text-sm font-bold text-hero-fg ring-1 ring-white/20">
      <span
        aria-hidden="true"
        class={`relative block h-2.5 w-2.5 rounded-full ${online ? 'bg-[#4ade80]' : offline ? 'bg-[#fca5a5]' : 'sb-pulse bg-accent'}`}
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
  return (
    <>
      <div class="sb-hero flex flex-col gap-3 px-5 pb-6 pt-5 shadow-float">
        <p class="text-sm font-extrabold uppercase tracking-wider text-hero-muted">
          {t('home.eyebrow')}
        </p>
        <div class="text-hero-fg [&_h1]:text-hero-fg">
          <PageTitle documentTitle={`${t('app.name')} · ${t('home.title')}`}>
            {t('home.title')}
          </PageTitle>
        </div>
        <p class="text-lg text-hero-muted">{t('home.subtitle')}</p>
        <StatusChip />
      </div>

      {helpRequested.value ? <IncidentStatus /> : null}

      <section aria-labelledby="channels-title" class="flex flex-col gap-3">
        <h2 id="channels-title" class="sr-only">
          {t('home.channels')}
        </h2>
        <ul class="grid gap-3">
          {OPTIONS.map((option, index) => (
            <li key={option.key} class="sb-rise" data-i={index}>
              <a
                href={option.href}
                class="group flex min-h-[5.5rem] items-center gap-4 rounded-card border-2 border-line-soft bg-surface p-4 shadow-card transition-[border-color,transform,box-shadow] hover:-translate-y-0.5 hover:border-line hover:shadow-float active:translate-y-0"
              >
                <span
                  aria-hidden="true"
                  class={`grid h-14 w-14 shrink-0 place-items-center rounded-2xl ${option.tint}`}
                >
                  {option.icon}
                </span>
                <span class="min-w-0 flex-1">
                  <span class="block font-display text-xl font-extrabold leading-snug">
                    {t(`channel.${option.key}.title` as MessageKey)}
                  </span>
                  <span class="mt-0.5 block text-base leading-snug text-muted">
                    {t(`channel.${option.key}.desc` as MessageKey)}
                  </span>
                </span>
                <ChevronRight
                  size={24}
                  aria-hidden="true"
                  class="shrink-0 text-muted transition-transform group-hover:translate-x-1"
                />
              </a>
            </li>
          ))}
        </ul>
      </section>

      {/* Aviso de privacidad: visible, breve y NUNCA bloquea pedir ayuda. */}
      <section
        aria-labelledby="privacy-short"
        class="flex items-start gap-3 rounded-card bg-surface-2 p-4 text-sm"
      >
        <span
          aria-hidden="true"
          class="grid h-9 w-9 shrink-0 place-items-center rounded-xl bg-surface text-fg"
        >
          <ShieldCheck size={20} />
        </span>
        <div class="min-w-0">
          <p id="privacy-short" class="text-base text-fg">
            {t('privacy.short')}
          </p>
          <a
            href="/aviso-de-privacidad"
            class="inline-flex min-h-touch items-center font-bold text-fg underline decoration-2 underline-offset-4"
          >
            {t('privacy.link')}
          </a>
        </div>
      </section>

      <section
        aria-labelledby="fallback-title"
        class="rounded-card border-2 border-dashed border-line p-4"
      >
        <h2
          id="fallback-title"
          class="mb-1 flex items-center gap-2 font-display text-lg font-extrabold"
        >
          <HandHelping size={22} aria-hidden="true" />
          {t('fallback.title')}
        </h2>
        <FallbackContact compact />
      </section>
    </>
  );
}
