import { ArrowLeft, Settings, ShieldCheck } from 'lucide-preact';
import type { ComponentChildren } from 'preact';
import { t } from '../i18n';
import { announcement, connectionStatus } from '../state/conversation';
import { ConnectionBanner } from './ConnectionBanner';
import { BrandMark } from './ui';

/**
 * Estructura común: enlace para saltar al contenido, barra superior, contenido
 * principal y región `aria-live` única para avisos (mensajes nuevos del C5 y
 * cambios de estado). Una sola región evita que el lector de pantalla repita.
 *
 * `minimal` (pantalla neutral de la salida rápida): sin barra, sin pie y sin
 * anuncios, para que nada delate la emergencia.
 */
export function Layout({
  children,
  minimal = false,
  home = false,
  conversation = false,
}: {
  children: ComponentChildren;
  minimal?: boolean;
  home?: boolean;
  /** Chat, modo silencioso o llamada: el cuadro de mensaje cierra la pantalla. */
  conversation?: boolean;
}) {
  return (
    <div class="mx-auto flex min-h-dvh max-w-2xl flex-col">
      <a
        href="#main"
        class="sr-only focus:not-sr-only focus:absolute focus:left-3 focus:top-3 focus:z-50 focus:rounded-button focus:bg-accent focus:p-3 focus:font-bold focus:text-on-accent"
      >
        {t('app.skip')}
      </a>
      {minimal ? null : (
        <header class="sticky top-0 z-20 flex items-center justify-between gap-3 bg-bg/90 px-4 py-2 backdrop-blur-md">
          {home ? (
            <a href="/" class="flex min-h-touch items-center gap-3 rounded-button pr-2">
              <BrandMark />
              <span class="leading-tight">
                <span class="block font-display text-lg font-black tracking-tight">
                  {t('app.name')}
                </span>
                <span class="block text-xs font-bold uppercase tracking-wider text-muted">
                  C5 Zapopan
                </span>
              </span>
            </a>
          ) : (
            <a
              href="/"
              class="inline-flex min-h-touch items-center gap-2 rounded-button pr-3 font-extrabold"
            >
              <span class="grid h-10 w-10 place-items-center rounded-full border-2 border-line-soft bg-surface shadow-card">
                <ArrowLeft size={20} aria-hidden="true" />
              </span>
              {t('app.home')}
            </a>
          )}
          <a
            href="/ajustes"
            class="inline-flex min-h-touch min-w-touch items-center justify-center gap-2 rounded-full border-2 border-line-soft bg-surface px-3 font-bold shadow-card"
          >
            <Settings size={20} aria-hidden="true" />
            <span class="hidden sm:inline">{t('app.settings')}</span>
            <span class="sr-only sm:hidden">{t('app.settings')}</span>
          </a>
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
        class={`flex flex-1 flex-col gap-5 px-4 pt-3 outline-none ${conversation ? '' : 'pb-6'}`}
      >
        {children}
      </main>
      {minimal || conversation ? null : (
        <footer class="mx-4 mb-6 flex flex-col gap-1 border-t border-line-soft pt-4 text-sm text-muted">
          <a
            href="/aviso-de-privacidad"
            class="inline-flex min-h-touch items-center gap-2 self-start font-bold text-fg underline decoration-2 underline-offset-4"
          >
            <ShieldCheck size={18} aria-hidden="true" />
            {t('privacy.link')}
          </a>
          <p>{t('app.program')}</p>
        </footer>
      )}
      <div aria-live="polite" aria-atomic="true" class="sr-only">
        {!minimal && announcement.value ? (
          <span key={announcement.value.id}>{announcement.value.text}</span>
        ) : null}
      </div>
    </div>
  );
}
