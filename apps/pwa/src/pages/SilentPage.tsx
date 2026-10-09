import { LogOut } from 'lucide-preact';
import { useEffect } from 'preact/hooks';
import { useLocation } from 'preact-iso';
import { Composer, MessageList } from '../components/Chat';
import { IncidentStatus } from '../components/IncidentStatus';
import { PageTitle } from '../components/ui';
import { type MessageKey, t } from '../i18n';
import type { QuickReply } from '../protocol/generated';
import { quickExit, requestHelp, sendQuickReply } from '../services';
import { silentMode } from '../state/settings';

const QUICK: QuickReply[] = ['in_danger', 'cannot_speak', 'being_followed', 'injured'];

/**
 * Modo silencioso: sin sonidos ni vibración (la app nunca los usa), colores
 * oscuros y discretos, respuestas rápidas y salida rápida a una pantalla neutral.
 */
export function SilentPage() {
  const { route } = useLocation();
  useEffect(() => {
    silentMode.value = true;
    requestHelp('silent');
    return () => {
      silentMode.value = false;
    };
  }, []);
  const exit = () => {
    quickExit();
    // `replace`: el botón "atrás" no regresa a esta pantalla.
    route('/clima', true);
  };
  return (
    <>
      <div class="flex items-start justify-between gap-3">
        <PageTitle documentTitle={t('app.name')}>{t('silent.title')}</PageTitle>
        <button
          type="button"
          onClick={exit}
          class="inline-flex min-h-touch shrink-0 items-center gap-2 rounded-button border-2 border-line px-4 font-bold"
          aria-describedby="exit-hint"
        >
          <LogOut size={20} />
          {t('silent.exit')}
        </button>
      </div>
      <p id="exit-hint" class="text-sm text-muted">
        {t('silent.exit_hint')}
      </p>
      <IncidentStatus />
      <p class="text-lg">{t('silent.hint')}</p>
      <div class="grid grid-cols-2 gap-3">
        {QUICK.map((code) => (
          <button
            key={code}
            type="button"
            onClick={() => sendQuickReply(code)}
            class="min-h-[5.5rem] rounded-card bg-primary p-3 text-lg font-extrabold text-on-primary"
          >
            {t(`silent.quick.${code}` as MessageKey)}
          </button>
        ))}
      </div>
      <MessageList />
      <Composer />
    </>
  );
}
