import { CheckCheck, Clock, Siren } from 'lucide-preact';
import { t } from '../i18n';
import { helpRequested, incident } from '../state/conversation';
import { Notice } from './ui';

/** "Te estamos atendiendo", "Unidad en camino (≈ 8 min)", "Incidente cerrado"… */
export function IncidentStatus() {
  const current = incident.value;
  if (!current) {
    if (!helpRequested.value) return null;
    return (
      <Notice icon={<span class="sb-pulse block h-3 w-3 rounded-full bg-accent" />}>
        <p class="font-bold">{t('incident.requesting')}</p>
      </Notice>
    );
  }
  const text =
    current.status === 'unit_en_route' && current.etaSeconds
      ? t('incident.unit_en_route_eta', {
          minutes: Math.max(1, Math.round(current.etaSeconds / 60)),
        })
      : t(`incident.${current.status}`);
  const icon =
    current.status === 'closed' ? (
      <CheckCheck size={22} />
    ) : current.status === 'unit_en_route' || current.status === 'unit_on_scene' ? (
      <Siren size={22} />
    ) : (
      <Clock size={22} />
    );
  return (
    <Notice tone={current.status === 'closed' ? 'success' : 'info'} icon={icon}>
      <p class="text-lg font-extrabold">{text}</p>
    </Notice>
  );
}
