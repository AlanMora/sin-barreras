import { MessageSquareText, Phone } from 'lucide-preact';
import { FALLBACK_PHONE, formatPhone } from '../config';
import { t } from '../i18n';

/** Respaldo cuando no hay conexión: llamada o SMS al número del programa. */
export function FallbackContact({ compact = false }: { compact?: boolean }) {
  const phone = formatPhone(FALLBACK_PHONE);
  return (
    <div class={compact ? 'mt-2' : 'flex flex-col gap-3'}>
      {compact ? null : <h2 class="font-display text-xl font-extrabold">{t('fallback.title')}</h2>}
      <div class="flex flex-wrap gap-2">
        <a
          href={`tel:${FALLBACK_PHONE}`}
          class="inline-flex min-h-touch items-center gap-2 rounded-full bg-primary px-5 font-extrabold text-on-primary shadow-card"
        >
          <Phone size={20} aria-hidden="true" />
          {t('fallback.call', { phone })}
        </a>
        <a
          href={`sms:${FALLBACK_PHONE}`}
          class="inline-flex min-h-touch items-center gap-2 rounded-full border-2 border-line bg-surface px-5 font-bold"
        >
          <MessageSquareText size={20} aria-hidden="true" />
          {t('fallback.sms', { phone })}
        </a>
      </div>
    </div>
  );
}
