import { PageTitle } from '../components/ui';
import { t } from '../i18n';

export function NotFound() {
  return (
    <>
      <PageTitle documentTitle={t('notfound.title')}>{t('notfound.title')}</PageTitle>
      <a
        href="/"
        class="inline-flex min-h-touch items-center text-lg font-bold text-accent-text underline"
      >
        {t('notfound.home')}
      </a>
    </>
  );
}
