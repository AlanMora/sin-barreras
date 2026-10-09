import { RefreshCw, WifiOff } from 'lucide-preact';
import { t } from '../i18n';
import { connectionStatus } from '../state/conversation';
import { FallbackContact } from './FallbackContact';
import { Button, Notice } from './ui';

/** Estado de la conexión, siempre visible si no estamos conectados. */
export function ConnectionBanner() {
  const status = connectionStatus.value;
  if (status === 'online') return null;
  if (status === 'outdated') {
    return (
      <Notice tone="warning" icon={<RefreshCw size={22} />} live>
        <p class="font-bold">{t('connection.outdated')}</p>
        <Button variant="accent" class="mt-3" onClick={() => location.reload()}>
          {t('connection.reload')}
        </Button>
      </Notice>
    );
  }
  const offline = status === 'offline' || status === 'reconnecting';
  return (
    <Notice
      tone={offline ? 'warning' : 'info'}
      icon={
        offline ? (
          <WifiOff size={22} />
        ) : (
          <span class="sb-pulse block h-3 w-3 rounded-full bg-accent" />
        )
      }
      live
    >
      <p class="font-bold">{t(`connection.${status}`)}</p>
      {offline ? <FallbackContact compact /> : null}
    </Notice>
  );
}
