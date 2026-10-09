import {
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
import { helpRequested } from '../state/conversation';

interface Option {
  href: string;
  key: 'lsm_video' | 'text_chat' | 'audio' | 'translation' | 'silent';
  icon: ComponentChildren;
}

const OPTIONS: Option[] = [
  { href: '/llamada?m=video', key: 'lsm_video', icon: <Video size={30} /> },
  { href: '/chat', key: 'text_chat', icon: <MessageSquareText size={30} /> },
  { href: '/llamada?m=audio', key: 'audio', icon: <Phone size={30} /> },
  { href: '/idioma', key: 'translation', icon: <Languages size={30} /> },
  { href: '/silencioso', key: 'silent', icon: <VolumeX size={30} /> },
];

export function Home() {
  useEffect(() => {
    // El aviso está visible en esta pantalla desde el primer momento.
    clientEvent('privacy_notice_shown');
    clientEvent('channel_screen_shown');
  }, []);
  return (
    <>
      <div class="flex flex-col gap-2">
        <PageTitle documentTitle={`${t('app.name')} · ${t('home.title')}`}>
          {t('home.title')}
        </PageTitle>
        <p class="text-lg text-muted">{t('home.subtitle')}</p>
      </div>

      {helpRequested.value ? <IncidentStatus /> : null}

      <ul class="grid gap-3">
        {OPTIONS.map((option) => (
          <li key={option.key}>
            <a
              href={option.href}
              class="flex min-h-[5rem] items-center gap-4 rounded-card border-2 border-line bg-surface p-4 transition-colors hover:border-accent"
            >
              <span class="grid h-14 w-14 shrink-0 place-items-center rounded-2xl bg-primary text-on-primary">
                {option.icon}
              </span>
              <span class="min-w-0">
                <span class="block font-display text-xl font-extrabold">
                  {t(`channel.${option.key}.title` as MessageKey)}
                </span>
                <span class="block text-base text-muted">
                  {t(`channel.${option.key}.desc` as MessageKey)}
                </span>
              </span>
            </a>
          </li>
        ))}
      </ul>

      {/* Aviso de privacidad: visible, breve y NUNCA bloquea pedir ayuda. */}
      <section
        aria-labelledby="privacy-short"
        class="flex items-start gap-3 rounded-card bg-surface p-4 text-sm"
      >
        <ShieldCheck size={20} class="mt-0.5 shrink-0" />
        <div>
          <p id="privacy-short">{t('privacy.short')}</p>
          <a
            href="/aviso-de-privacidad"
            class="inline-flex min-h-touch items-center font-bold underline"
          >
            {t('privacy.link')}
          </a>
        </div>
      </section>

      <section
        aria-label={t('fallback.title')}
        class="rounded-card border-2 border-dashed border-line p-4"
      >
        <p class="mb-3 flex items-center gap-2 font-bold">
          <HandHelping size={20} />
          {t('fallback.title')}
        </p>
        <FallbackContact compact />
      </section>
    </>
  );
}
