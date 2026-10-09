import { Settings, ShieldCheck } from 'lucide-preact';
import type { ComponentChildren } from 'preact';
import { t } from '../i18n';
import { announcement, connectionStatus } from '../state/conversation';
import { ConnectionBanner } from './ConnectionBanner';

/**
 * Estructura común: enlace para saltar al contenido, encabezado, contenido
 * principal y región `aria-live` única para avisos (mensajes nuevos del C5 y
 * cambios de estado). Una sola región evita que el lector de pantalla repita.
 */
export function Layout({
  children,
  minimal = false,
}: {
  children: ComponentChildren;
  minimal?: boolean;
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
        <header class="flex items-center justify-between gap-3 px-4 pt-4">
          <a href="/" class="flex min-h-touch items-center gap-2 rounded-button pr-2">
            <span
              aria-hidden="true"
              class="grid h-10 w-10 place-items-center rounded-xl bg-primary"
            >
              <span class="block h-5 w-1.5 rounded bg-mark" />
              <span class="absolute block h-1.5 w-5 rounded bg-mark" />
            </span>
            <span class="leading-tight">
              <span class="block font-display text-lg font-extrabold">{t('app.name')}</span>
              <span class="block text-xs text-muted">C5 Zapopan</span>
            </span>
          </a>
          <a
            href="/ajustes"
            class="inline-flex min-h-touch min-w-touch items-center justify-center gap-2 rounded-button border-2 border-line px-3 font-bold"
          >
            <Settings size={22} />
            <span class="hidden sm:inline">{t('app.settings')}</span>
            <span class="sr-only sm:hidden">{t('app.settings')}</span>
          </a>
        </header>
      )}
      {minimal || connectionStatus.value === 'online' ? null : (
        <div class="px-4 pt-3">
          <ConnectionBanner />
        </div>
      )}
      <main id="main" tabIndex={-1} class="flex flex-1 flex-col gap-6 px-4 py-5 outline-none">
        {children}
      </main>
      {minimal ? null : (
        <footer class="px-4 pb-6 text-sm text-muted">
          <a
            href="/aviso-de-privacidad"
            class="inline-flex min-h-touch items-center gap-2 underline"
          >
            <ShieldCheck size={18} />
            {t('privacy.link')}
          </a>
        </footer>
      )}
      <div aria-live="polite" aria-atomic="true" class="sr-only">
        {announcement.value ? (
          <span key={announcement.value.id}>{announcement.value.text}</span>
        ) : null}
      </div>
    </div>
  );
}
