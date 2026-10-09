import { MapPin } from 'lucide-preact';
import { t } from '../i18n';
import { locationStatus } from '../location/location';
import { entryVerification } from '../state/conversation';
import { Notice } from './ui';

export function LocationNotice() {
  const status = locationStatus.value;
  if (status === 'sharing') {
    return (
      <p class="flex items-center gap-2 text-sm text-muted">
        <MapPin size={18} />
        {t('location.shared')}
      </p>
    );
  }
  if (status === 'denied' || status === 'unavailable') {
    return (
      <Notice icon={<MapPin size={22} />}>
        <p>{t('location.denied')}</p>
        {entryVerification.value !== 'none' ? (
          <p class="mt-1 text-sm text-muted">{t('location.qr')}</p>
        ) : null}
      </Notice>
    );
  }
  return null;
}
