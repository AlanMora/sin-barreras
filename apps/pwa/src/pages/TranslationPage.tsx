import { useLocation } from 'preact-iso';
import { PageTitle } from '../components/ui';
import { t } from '../i18n';
import { setLocale } from '../state/settings';

/** Idiomas más frecuentes en Zapopan y lenguas originarias de la región. */
const LANGUAGES: { code: string; name: string }[] = [
  { code: 'en', name: 'English' },
  { code: 'fr', name: 'Français' },
  { code: 'de', name: 'Deutsch' },
  { code: 'pt', name: 'Português' },
  { code: 'it', name: 'Italiano' },
  { code: 'zh', name: '中文' },
  { code: 'ja', name: '日本語' },
  { code: 'ko', name: '한국어' },
  { code: 'ar', name: 'العربية' },
  { code: 'hch', name: 'Wixárika' },
  { code: 'nah', name: 'Náhuatl' },
  { code: 'tsz', name: 'P’urhépecha' },
];

export function TranslationPage() {
  const { route } = useLocation();
  const choose = (code: string) => {
    if (code === 'en') setLocale('en');
    route(`/chat?lang=${encodeURIComponent(code)}`);
  };
  return (
    <>
      <PageTitle documentTitle={t('translation.title')}>{t('translation.title')}</PageTitle>
      <p class="text-lg text-muted">{t('translation.hint')}</p>
      <ul class="grid grid-cols-2 gap-3">
        {LANGUAGES.map((lang) => (
          <li key={lang.code}>
            <button
              type="button"
              lang={lang.code}
              onClick={() => choose(lang.code)}
              class="min-h-touch w-full rounded-card border-2 border-line bg-surface p-4 text-left text-xl font-extrabold"
            >
              {lang.name}
            </button>
          </li>
        ))}
      </ul>
    </>
  );
}
