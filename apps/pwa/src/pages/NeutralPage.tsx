import { Cloud, CloudRain, CloudSun, Droplets, Sun, Wind } from 'lucide-preact';
import type { ComponentChildren } from 'preact';
import { useEffect } from 'preact/hooks';
import { locale, t } from '../i18n';

/** Pronóstico de muestra (fijo): la pantalla solo debe parecer una app común. */
const HOURS: { offset: number; temp: number; icon: ComponentChildren }[] = [
  { offset: 1, temp: 25, icon: <Sun size={24} /> },
  { offset: 2, temp: 25, icon: <CloudSun size={24} /> },
  { offset: 3, temp: 23, icon: <Cloud size={24} /> },
  { offset: 4, temp: 21, icon: <CloudRain size={24} /> },
  { offset: 5, temp: 20, icon: <Cloud size={24} /> },
];

/**
 * Pantalla neutral de la salida rápida. No menciona emergencias. La sesión y
 * la conversación siguen activas; "Ver pronóstico" regresa discretamente.
 */
export function NeutralPage() {
  useEffect(() => {
    document.title = t('neutral.title');
  }, []);
  const now = new Date();
  const day = new Intl.DateTimeFormat(locale.value, {
    weekday: 'long',
    day: 'numeric',
    month: 'long',
  }).format(now);
  const hour = new Intl.DateTimeFormat(locale.value, { hour: 'numeric' });
  return (
    <div class="flex flex-col gap-5 pt-4">
      <div>
        <h1 tabIndex={-1} class="font-display text-3xl font-black tracking-tight">
          {t('neutral.title')}
        </h1>
        <p class="text-lg text-muted first-letter:uppercase">{day}</p>
      </div>
      <div class="sb-card flex flex-col gap-5 p-6">
        <div class="flex items-center justify-between gap-4">
          <div>
            <p class="text-6xl font-black tracking-tight">24°</p>
            <p class="mt-1 text-lg font-bold">{t('neutral.weather')}</p>
            <p class="text-muted">{t('neutral.today')} · 27° / 16°</p>
          </div>
          <CloudSun size={84} aria-hidden="true" class="shrink-0 text-muted" />
        </div>
        <dl class="grid grid-cols-3 gap-2 text-center">
          <div class="rounded-2xl bg-surface-2 p-3">
            <dt class="flex items-center justify-center gap-1 text-sm text-muted">
              <Droplets size={16} aria-hidden="true" />
              {t('neutral.humidity')}
            </dt>
            <dd class="text-lg font-extrabold">48%</dd>
          </div>
          <div class="rounded-2xl bg-surface-2 p-3">
            <dt class="flex items-center justify-center gap-1 text-sm text-muted">
              <Wind size={16} aria-hidden="true" />
              {t('neutral.wind')}
            </dt>
            <dd class="text-lg font-extrabold">12 km/h</dd>
          </div>
          <div class="rounded-2xl bg-surface-2 p-3">
            <dt class="flex items-center justify-center gap-1 text-sm text-muted">
              <CloudRain size={16} aria-hidden="true" />
              {t('neutral.rain')}
            </dt>
            <dd class="text-lg font-extrabold">30%</dd>
          </div>
        </dl>
      </div>
      <section class="sb-card p-5">
        <h2 class="mb-3 font-display text-lg font-extrabold">{t('neutral.hourly')}</h2>
        <ul class="grid grid-cols-5 gap-1 text-center">
          {HOURS.map((h) => (
            <li key={h.offset} class="flex flex-col items-center gap-1">
              <span class="text-sm text-muted">
                {hour.format(new Date(now.getTime() + h.offset * 3_600_000))}
              </span>
              <span aria-hidden="true">{h.icon}</span>
              <span class="font-extrabold">{h.temp}°</span>
            </li>
          ))}
        </ul>
      </section>
      <a
        href="/silencioso"
        class="inline-flex min-h-touch items-center self-start text-lg font-bold underline decoration-2 underline-offset-4"
      >
        {t('neutral.more')}
      </a>
    </div>
  );
}
