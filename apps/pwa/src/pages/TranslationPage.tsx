import { ChevronRight, Globe } from 'lucide-preact';
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
      <div class="flex items-start gap-4">
        <span
          aria-hidden="true"
          class="grid h-14 w-14 shrink-0 place-items-center rounded-2xl sb-tint-4"
        >
          <Globe size={28} />
        </span>
        <div class="flex flex-col gap-1">
          <PageTitle documentTitle={t('translation.title')}>{t('translation.title')}</PageTitle>
          <p class="text-lg text-muted">{t('translation.hint')}</p>
        </div>
      </div>
      <ul class="grid grid-cols-2 gap-3">
        {LANGUAGES.map((lang) => (
          <li key={lang.code}>
            <button
              type="button"
              lang={lang.code}
              onClick={() => choose(lang.code)}
              class="flex min-h-[4rem] w-full items-center justify-between gap-2 rounded-card border-2 border-line-soft bg-surface px-4 py-3 text-left text-lg font-extrabold shadow-card transition-colors hover:border-line"
            >
              <span class="min-w-0 break-words">{lang.name}</span>
              <ChevronRight size={20} aria-hidden="true" class="shrink-0 text-muted" />
            </button>
          </li>
        ))}
      </ul>
    </>
  );
}
