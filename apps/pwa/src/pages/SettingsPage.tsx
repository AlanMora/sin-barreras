import { Contrast, Globe, Type } from 'lucide-preact';
import type { ComponentChildren } from 'preact';
import { PageTitle } from '../components/ui';
import { LOCALES, type Locale, locale, t } from '../i18n';
import { clientEvent } from '../services';
import { FONT_SCALES, type FontScale, fontScale, highContrast, setLocale } from '../state/settings';

const FONT_KEYS = {
  1: 'settings.font.100',
  1.25: 'settings.font.125',
  1.5: 'settings.font.150',
  2: 'settings.font.200',
} as const;

/** Tamaño de la muestra "Aa" de cada opción (clases fijas: sin estilos en línea). */
const FONT_PREVIEW = {
  1: 'text-base',
  1.25: 'text-xl',
  1.5: 'text-2xl',
  2: 'text-3xl',
} as const;

function Section({
  icon,
  tint,
  title,
  children,
}: {
  icon: ComponentChildren;
  tint: string;
  title: string;
  children: ComponentChildren;
}) {
  return (
    <fieldset class="sb-card flex flex-col gap-3 p-4">
      <legend class="sr-only">{title}</legend>
      <p aria-hidden="true" class="flex items-center gap-3 font-display text-lg font-extrabold">
        <span class={`grid h-10 w-10 place-items-center rounded-xl ${tint}`}>{icon}</span>
        {title}
      </p>
      {children}
    </fieldset>
  );
}

/** Ajustes con controles nativos (radios y casilla): accesibles por defecto. */
export function SettingsPage() {
  const changed = (detail: string) => clientEvent('settings_changed', detail);
  return (
    <>
      <div class="flex flex-col gap-1">
        <PageTitle documentTitle={`${t('settings.title')} · ${t('app.name')}`}>
          {t('settings.title')}
        </PageTitle>
        <p class="text-muted">{t('settings.saved')}</p>
      </div>

      <Section icon={<Globe size={20} />} tint="sb-tint-4" title={t('settings.language')}>
        <div class="grid grid-cols-2 gap-2">
          {(Object.keys(LOCALES) as Locale[]).map((code) => (
            <label key={code} class="sb-option">
              <input
                type="radio"
                name="language"
                class="sb-choice"
                checked={locale.value === code}
                onChange={() => {
                  setLocale(code);
                  changed(`language=${code}`);
                }}
              />
              <span lang={code} class="text-lg font-bold">
                {LOCALES[code].label}
              </span>
            </label>
          ))}
        </div>
      </Section>

      <Section icon={<Type size={20} />} tint="sb-tint-2" title={t('settings.font')}>
        <div class="grid grid-cols-2 gap-2">
          {FONT_SCALES.map((scale) => (
            <label key={scale} class="sb-option">
              <input
                type="radio"
                name="font"
                class="sb-choice"
                aria-labelledby={`font-${scale * 100}`}
                checked={fontScale.value === scale}
                onChange={() => {
                  fontScale.value = scale as FontScale;
                  changed(`font_scale=${scale}`);
                }}
              />
              <span class="flex min-w-0 flex-col leading-tight">
                <span aria-hidden="true" class={`font-display font-black ${FONT_PREVIEW[scale]}`}>
                  Aa
                </span>
                <span id={`font-${scale * 100}`} class="font-bold">
                  {t(FONT_KEYS[scale])}
                </span>
              </span>
            </label>
          ))}
        </div>
        <p class="rounded-2xl bg-surface-2 px-4 py-3 text-lg">{t('settings.preview')}</p>
      </Section>

      <Section icon={<Contrast size={20} />} tint="sb-tint-5" title={t('settings.appearance')}>
        <label class="sb-option justify-between">
          <span>
            <span class="block text-lg font-bold">{t('settings.contrast')}</span>
            <span id="contrast-hint" class="block text-muted">
              {t('settings.contrast_hint')}
            </span>
          </span>
          <input
            type="checkbox"
            class="sb-switch"
            checked={highContrast.value}
            aria-describedby="contrast-hint"
            onChange={(e) => {
              highContrast.value = e.currentTarget.checked;
              changed(`high_contrast=${e.currentTarget.checked}`);
            }}
          />
        </label>
      </Section>
    </>
  );
}
