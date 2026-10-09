import { useEffect } from 'preact/hooks';
import { useLocation } from 'preact-iso';
import { Composer, MessageList } from '../components/Chat';
import { IncidentStatus } from '../components/IncidentStatus';
import { LocationNotice } from '../components/LocationNotice';
import { PageTitle } from '../components/ui';
import { t } from '../i18n';
import { requestHelp } from '../services';

export function ChatPage() {
  const { query } = useLocation();
  const language = query.lang;
  useEffect(() => {
    requestHelp(language ? 'translation' : 'text_chat', language);
  }, [language]);
  return (
    <>
      <PageTitle documentTitle={`${t('chat.title')} · ${t('app.name')}`}>
        {t('chat.title')}
      </PageTitle>
      <IncidentStatus />
      <LocationNotice />
      <section aria-label={t('chat.log')} class="flex flex-1 flex-col gap-3">
        <MessageList suggestions />
      </section>
      <Composer />
    </>
  );
}
