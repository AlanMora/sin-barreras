import { Bandage, Footprints, LogOut, MicOff, TriangleAlert } from 'lucide-preact';
import type { ComponentChildren } from 'preact';
import { useEffect } from 'preact/hooks';
import { useLocation } from 'preact-iso';
import { Composer, MessageList } from '../components/Chat';
import { IncidentStatus } from '../components/IncidentStatus';
import { PageTitle } from '../components/ui';
import { type MessageKey, t } from '../i18n';
import type { QuickReply } from '../protocol/generated';
import { quickExit, requestHelp, sendQuickReply } from '../services';
import { silentMode } from '../state/settings';

const QUICK: { code: QuickReply; icon: ComponentChildren }[] = [
  { code: 'in_danger', icon: <TriangleAlert size={26} /> },
  { code: 'cannot_speak', icon: <MicOff size={26} /> },
  { code: 'being_followed', icon: <Footprints size={26} /> },
  { code: 'injured', icon: <Bandage size={26} /> },
];

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
      <div class="flex items-center justify-between gap-3">
        <PageTitle documentTitle={t('app.name')}>{t('silent.title')}</PageTitle>
        <button
          type="button"
          onClick={exit}
          class="inline-flex min-h-touch shrink-0 items-center gap-2 rounded-full border-2 border-line bg-surface px-4 font-bold"
          aria-describedby="exit-hint"
        >
          <LogOut size={20} aria-hidden="true" />
          {t('silent.exit')}
        </button>
      </div>
      <p id="exit-hint" class="-mt-3 text-sm text-muted">
        {t('silent.exit_hint')}
      </p>
      <IncidentStatus />
      <p class="text-lg font-bold">{t('silent.hint')}</p>
      <div class="grid grid-cols-2 gap-3">
        {QUICK.map(({ code, icon }) => (
          <button
            key={code}
            type="button"
            onClick={() => sendQuickReply(code)}
            class="flex min-h-[6.5rem] flex-col items-start justify-between gap-3 rounded-card border-2 border-line-soft bg-surface p-4 text-left text-lg font-extrabold leading-tight text-fg transition-transform active:scale-[0.98]"
          >
            <span
              aria-hidden="true"
              class="grid h-11 w-11 place-items-center rounded-xl bg-primary text-on-primary"
            >
              {icon}
            </span>
            {t(`silent.quick.${code}` as MessageKey)}
          </button>
        ))}
      </div>
      <MessageList />
      <Composer />
    </>
  );
}
