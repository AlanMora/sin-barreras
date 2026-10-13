import { ArrowLeft, ClipboardList, Globe, House, Info, Settings } from 'lucide-preact';
import type { ComponentChildren } from 'preact';
import { LOCALES, type Locale, locale, t } from '../i18n';
import { clientEvent } from '../services';
import { announcement, connectionStatus } from '../state/conversation';
import { setLocale } from '../state/settings';
import { ConnectionBanner } from './ConnectionBanner';
import { Wordmark } from './ui';

/** Cambio rápido de idioma (para personas extranjeras): muestra el idioma al
 *  que se cambia, no el actual. */
function LanguageToggle() {
  const next: Locale = locale.value === 'en' ? 'es-MX' : 'en';
  const short = next === 'en' ? 'EN' : 'ES';
  return (
    <button
      type="button"
      lang={next}
      onClick={() => {
        setLocale(next);
        clientEvent('settings_changed', `language=${next}`);
      }}
      class="inline-flex min-h-touch min-w-touch items-center justify-center gap-1.5 rounded-full border border-glass bg-surface px-3 font-extrabold"
    >
      <Globe size={18} aria-hidden="true" />
      <span>{short}</span>
      <span class="sr-only">
        {' '}
        · {t('lang.switch_to')} {LOCALES[next].label}
      </span>
    </button>
  );
}

const NAV = [
  { href: '/', key: 'nav.home', icon: House },
  { href: '/mis-reportes', key: 'nav.reports', icon: ClipboardList },
  { href: '/aviso-de-privacidad', key: 'nav.info', icon: Info },
  { href: '/ajustes', key: 'nav.settings', icon: Settings },
] as const;

function BottomNav({ path }: { path: string }) {
  return (
    <nav aria-label={t('nav.label')} class="sb-nav fixed inset-x-0 bottom-0 z-30">
      <ul class="mx-auto grid max-w-2xl grid-cols-4">
        {NAV.map(({ href, key, icon: Icon }) => {
          const current = path === href;
          return (
            <li key={href}>
              <a
                href={href}
                {...(current ? { 'aria-current': 'page' } : {})}
                class={`flex min-h-[4rem] flex-col items-center justify-center gap-1 px-1 text-center text-sm font-bold leading-tight ${current ? 'text-accent-text' : 'text-muted'}`}
              >
                <Icon size={24} aria-hidden="true" strokeWidth={current ? 2.5 : 2} />
                {t(key)}
              </a>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}

/**
 * Estructura común: enlace para saltar al contenido, barra superior, contenido
 * principal, navegación inferior y región `aria-live` única para avisos
 * (mensajes nuevos del C5 y cambios de estado). Una sola región evita que el
 * lector de pantalla repita.
 *
 * `minimal` (pantalla neutral de la salida rápida): sin barras y sin anuncios,
 * para que nada delate la emergencia. `conversation` (chat, silencioso,
 * llamada): sin navegación inferior; el cuadro de mensaje cierra la pantalla.
 */
export function Layout({
  children,
  path,
  minimal = false,
  conversation = false,
}: {
  children: ComponentChildren;
  path: string;
  minimal?: boolean;
  conversation?: boolean;
}) {
  const home = path === '/';
  const nav = !minimal && !conversation;
  return (
    <div class="mx-auto flex min-h-dvh max-w-2xl flex-col">
      <a
        href="#main"
        class="sr-only focus:not-sr-only focus:absolute focus:left-3 focus:top-3 focus:z-50 focus:rounded-button focus:bg-accent focus:p-3 focus:font-bold focus:text-on-accent"
      >
        {t('app.skip')}
      </a>
      {minimal ? null : (
        <header class="sticky top-0 z-20 flex items-center justify-between gap-3 bg-bg/70 px-4 py-2 backdrop-blur-md">
          {home ? (
            <a href="/" class="flex min-h-touch items-center rounded-button py-1 pr-2">
              <Wordmark />
            </a>
          ) : (
            <a
              href="/"
              class="inline-flex min-h-touch items-center gap-2 rounded-button pr-3 font-extrabold"
            >
              <span class="grid h-10 w-10 place-items-center rounded-full border border-glass bg-surface">
                <ArrowLeft size={20} aria-hidden="true" />
              </span>
              {t('app.home')}
            </a>
          )}
          <div class="flex items-center gap-2">
            <LanguageToggle />
            {conversation ? (
              <a
                href="/ajustes"
                class="inline-flex min-h-touch min-w-touch items-center justify-center rounded-full border border-glass bg-surface"
              >
                <Settings size={20} aria-hidden="true" />
                <span class="sr-only">{t('app.settings')}</span>
              </a>
            ) : null}
          </div>
        </header>
      )}
      {minimal || connectionStatus.value === 'online' ? null : (
        <div class="px-4 pt-2">
          <ConnectionBanner />
        </div>
      )}
      <main
        id="main"
        tabIndex={-1}
        class={`flex flex-1 flex-col gap-5 px-4 pt-3 outline-none ${nav ? 'sb-nav-space' : conversation ? '' : 'pb-6'}`}
      >
        {children}
      </main>
      {nav ? <BottomNav path={path} /> : null}
      <div aria-live="polite" aria-atomic="true" class="sr-only">
        {!minimal && announcement.value ? (
          <span key={announcement.value.id}>{announcement.value.text}</span>
        ) : null}
      </div>
    </div>
  );
}
