import { Compass } from 'lucide-preact';
import { PageTitle } from '../components/ui';
import { t } from '../i18n';

export function NotFound() {
  return (
    <div class="flex flex-col items-center gap-4 py-10 text-center">
      <span aria-hidden="true" class="grid h-16 w-16 place-items-center rounded-2xl sb-tint-2">
        <Compass size={32} />
      </span>
      <PageTitle documentTitle={t('notfound.title')}>{t('notfound.title')}</PageTitle>
      <a
        href="/"
        class="inline-flex min-h-touch items-center rounded-full bg-accent px-6 text-lg font-extrabold text-on-accent shadow-card"
      >
        {t('notfound.home')}
      </a>
    </div>
  );
}
