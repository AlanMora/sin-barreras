import { ChevronRight, ClipboardList, ShieldCheck } from 'lucide-preact';
import { IncidentStatus } from '../components/IncidentStatus';
import { PageTitle } from '../components/ui';
import { t } from '../i18n';
import { CONVERSATION_PATH, helpRequested } from '../state/conversation';

/**
 * Reporte actual del ciudadano. A propósito no hay historial: en un teléfono
 * compartido o revisado por un agresor, una lista de emergencias pasadas
 * pondría en riesgo a la persona (y no hay cuentas con qué protegerla).
 */
export function ReportsPage() {
  const active = helpRequested.value;
  return (
    <>
      <div class="flex flex-col gap-2 pt-1">
        <PageTitle documentTitle={`${t('app.name')} · ${t('reports.title')}`}>
          {t('reports.title')}
        </PageTitle>
        <p class="text-base text-muted sm:text-lg">{t('reports.subtitle')}</p>
      </div>

      {active ? (
        <div class="flex flex-col gap-3">
          <IncidentStatus />
          <a href={CONVERSATION_PATH[active]} class="sb-cta">
            {t('home.cta_back')}
            <ChevronRight size={28} strokeWidth={3} aria-hidden="true" />
          </a>
        </div>
      ) : (
        <div class="sb-card flex flex-col items-center gap-4 p-6 text-center">
          <span aria-hidden="true" class="grid h-14 w-14 place-items-center rounded-2xl sb-tint-2">
            <ClipboardList size={28} />
          </span>
          <div class="flex flex-col gap-1">
            <h2 class="font-display text-xl font-extrabold">{t('reports.empty.title')}</h2>
            <p class="text-muted">{t('reports.empty.body')}</p>
          </div>
          <a href="/chat" class="sb-cta">
            {t('home.cta')}
            <ChevronRight size={28} strokeWidth={3} aria-hidden="true" />
          </a>
        </div>
      )}

      <p class="flex items-start gap-2 text-sm text-muted">
        <ShieldCheck size={18} aria-hidden="true" class="mt-0.5 shrink-0" />
        {t('reports.safety')}
      </p>
    </>
  );
}
