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

/** Ajustes con controles nativos (radios y casilla): accesibles por defecto. */
export function SettingsPage() {
  const changed = (detail: string) => clientEvent('settings_changed', detail);
  return (
    <>
      <PageTitle documentTitle={`${t('settings.title')} · ${t('app.name')}`}>
        {t('settings.title')}
      </PageTitle>

      <fieldset class="flex flex-col gap-2">
        <legend class="mb-2 font-display text-xl font-extrabold">{t('settings.language')}</legend>
        {(Object.keys(LOCALES) as Locale[]).map((code) => (
          <label
            key={code}
            class="flex min-h-touch cursor-pointer items-center gap-3 rounded-button border-2 border-line px-4"
          >
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
      </fieldset>

      <fieldset class="flex flex-col gap-2">
        <legend class="mb-2 font-display text-xl font-extrabold">{t('settings.font')}</legend>
        {FONT_SCALES.map((scale) => (
          <label
            key={scale}
            class="flex min-h-touch cursor-pointer items-center gap-3 rounded-button border-2 border-line px-4"
          >
            <input
              type="radio"
              name="font"
              class="sb-choice"
              checked={fontScale.value === scale}
              onChange={() => {
                fontScale.value = scale as FontScale;
                changed(`font_scale=${scale}`);
              }}
            />
            <span class="font-bold" style={{ fontSize: `${scale * 1.05}rem` }}>
              {t(FONT_KEYS[scale])}
            </span>
          </label>
        ))}
      </fieldset>

      <label class="flex min-h-touch cursor-pointer items-start gap-3 rounded-button border-2 border-line p-4">
        <input
          type="checkbox"
          class="sb-choice mt-1"
          checked={highContrast.value}
          aria-describedby="contrast-hint"
          onChange={(e) => {
            highContrast.value = e.currentTarget.checked;
            changed(`high_contrast=${e.currentTarget.checked}`);
          }}
        />
        <span>
          <span class="block text-lg font-bold">{t('settings.contrast')}</span>
          <span id="contrast-hint" class="block text-muted">
            {t('settings.contrast_hint')}
          </span>
        </span>
      </label>
    </>
  );
}
