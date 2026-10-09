import { MapPin } from 'lucide-preact';
import { t } from '../i18n';
import { locationStatus } from '../location/location';
import { entryVerification } from '../state/conversation';
import { Notice } from './ui';

export function LocationNotice() {
  const status = locationStatus.value;
  if (status === 'sharing') {
    return (
      <p class="inline-flex items-center gap-2 self-start rounded-full bg-success-soft px-3 py-1.5 text-sm font-bold text-success">
        <MapPin size={16} aria-hidden="true" />
        {t('location.shared')}
      </p>
    );
  }
  if (status === 'denied' || status === 'unavailable') {
    return (
      <Notice icon={<MapPin size={20} />}>
        <p class="font-bold">{t('location.denied')}</p>
        {entryVerification.value !== 'none' ? (
          <p class="mt-1 text-sm text-muted">{t('location.qr')}</p>
        ) : null}
      </Notice>
    );
  }
  return null;
}
