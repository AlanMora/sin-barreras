import { Check, CheckCheck, Clock, LoaderCircle, Siren } from 'lucide-preact';
import { type MessageKey, t } from '../i18n';
import type { IncidentState } from '../protocol/generated';
import { helpRequested, incident } from '../state/conversation';

const STEPS: { states: IncidentState[]; key: MessageKey }[] = [
  { states: ['received'], key: 'incident.step.received' },
  { states: ['attending'], key: 'incident.step.attending' },
  { states: ['unit_en_route'], key: 'incident.step.en_route' },
  { states: ['unit_on_scene', 'closed'], key: 'incident.step.on_scene' },
];

/** Avance visual del incidente. El texto principal ya dice el estado; la lista
 *  da contexto (qué pasó y qué sigue). */
function Stepper({ status }: { status: IncidentState }) {
  const current = STEPS.findIndex((step) => step.states.includes(status));
  return (
    <ol aria-label={t('incident.progress')} class="mt-4 grid grid-cols-4 gap-1.5">
      {STEPS.map((step, index) => {
        const done = index < current || status === 'closed';
        const active = index === current && status !== 'closed';
        return (
          <li
            key={step.key}
            {...(active ? { 'aria-current': 'step' } : {})}
            class="flex flex-col gap-1.5"
          >
            <span
              aria-hidden="true"
              class={`block h-2 rounded-full ${done ? 'bg-success' : active ? 'sb-pulse bg-mark' : 'border-2 border-line-soft'}`}
            />
            <span
              class={`text-xs leading-tight ${done || active ? 'font-extrabold text-fg' : 'text-muted'}`}
            >
              {t(step.key)}
              {done ? <span class="sr-only"> ({t('incident.step.done')})</span> : null}
              {active ? <span class="sr-only"> ({t('incident.step.current')})</span> : null}
            </span>
          </li>
        );
      })}
    </ol>
  );
}

/** "Te estamos atendiendo", "Unidad en camino (≈ 8 min)", "Incidente cerrado"… */
export function IncidentStatus() {
  const current = incident.value;
  if (!current) {
    if (!helpRequested.value) return null;
    return (
      <div class="sb-rise flex items-center gap-3 rounded-card border-2 border-line-soft bg-surface p-4 shadow-card">
        <span
          aria-hidden="true"
          class="grid h-10 w-10 shrink-0 place-items-center rounded-xl sb-tint-1"
        >
          <LoaderCircle size={22} class="sb-spin" />
        </span>
        <p class="font-bold">{t('incident.requesting')}</p>
      </div>
    );
  }
  const minutes =
    current.status === 'unit_en_route' && current.etaSeconds
      ? Math.max(1, Math.round(current.etaSeconds / 60))
      : null;
  const text = minutes
    ? t('incident.unit_en_route_eta', { minutes })
    : t(`incident.${current.status}`);
  const closed = current.status === 'closed';
  const moving = current.status === 'unit_en_route' || current.status === 'unit_on_scene';
  const icon = closed ? (
    <CheckCheck size={22} />
  ) : moving ? (
    <Siren size={22} />
  ) : current.status === 'attending' ? (
    <Check size={22} />
  ) : (
    <Clock size={22} />
  );
  return (
    <section
      class={`sb-rise rounded-card border-2 p-4 shadow-card ${closed ? 'border-success bg-success-soft' : 'border-line-soft bg-surface'}`}
    >
      <div class="flex items-start gap-3">
        <span
          aria-hidden="true"
          class={`grid h-11 w-11 shrink-0 place-items-center rounded-xl ${closed ? 'bg-success text-bg' : moving ? 'sb-tint-1' : 'sb-tint-3'}`}
        >
          {icon}
        </span>
        <p class="min-w-0 flex-1 self-center text-lg font-extrabold leading-snug">{text}</p>
        {minutes ? (
          <span
            aria-hidden="true"
            class="shrink-0 rounded-full bg-accent px-3 py-1 text-sm font-black text-on-accent"
          >
            {t('incident.eta', { minutes })}
          </span>
        ) : null}
      </div>
      <Stepper status={current.status} />
    </section>
  );
}
