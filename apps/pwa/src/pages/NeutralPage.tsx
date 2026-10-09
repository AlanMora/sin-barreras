import { CloudSun } from 'lucide-preact';
import { useEffect } from 'preact/hooks';
import { t } from '../i18n';

/**
 * Pantalla neutral de la salida rápida. No menciona emergencias. La sesión y
 * la conversación siguen activas; "Ver pronóstico" regresa discretamente.
 */
export function NeutralPage() {
  useEffect(() => {
    document.title = t('neutral.title');
  }, []);
  const day = new Intl.DateTimeFormat(undefined, { weekday: 'long' }).format(new Date());
  return (
    <div class="flex flex-col gap-6 pt-6">
      <h1 tabIndex={-1} class="font-display text-3xl font-extrabold">
        {t('neutral.title')}
      </h1>
      <div class="flex items-center gap-4 rounded-card bg-surface p-6">
        <CloudSun size={64} />
        <div>
          <p class="text-5xl font-extrabold">24°</p>
          <p class="text-lg capitalize text-muted">
            {t('neutral.today')} · {day}
          </p>
          <p class="text-lg">{t('neutral.weather')}</p>
        </div>
      </div>
      <a href="/silencioso" class="inline-flex min-h-touch items-center text-lg underline">
        {t('neutral.more')}
      </a>
    </div>
  );
}
