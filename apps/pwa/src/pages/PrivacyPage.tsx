import { Database, FileText, HeartPulse, Lock, Scale } from 'lucide-preact';
import type { ComponentChildren } from 'preact';
import { useEffect } from 'preact/hooks';
import { PageTitle } from '../components/ui';
import { t } from '../i18n';
import { clientEvent } from '../services';

function Block({
  icon,
  tint,
  title,
  children,
}: {
  icon: ComponentChildren;
  tint: string;
  title: string;
  children: ComponentChildren;
}) {
  return (
    <section class="sb-card flex flex-col gap-2 p-5">
      <h2 class="flex items-center gap-3 font-display text-xl font-extrabold">
        <span aria-hidden="true" class={`grid h-10 w-10 place-items-center rounded-xl ${tint}`}>
          {icon}
        </span>
        {title}
      </h2>
      {children}
    </section>
  );
}

/**
 * Aviso de privacidad integral. BORRADOR técnico: el texto definitivo lo
 * aprueba el área jurídica del municipio (LFPDPPP y normativa de sujetos
 * obligados). Ver docs/aviso-de-privacidad.md.
 */
export function PrivacyPage() {
  useEffect(() => clientEvent('privacy_notice_opened'), []);
  return (
    <article class="flex flex-col gap-4 text-lg leading-relaxed">
      <div class="flex items-start gap-4">
        <span
          aria-hidden="true"
          class="grid h-14 w-14 shrink-0 place-items-center rounded-2xl sb-tint-2"
        >
          <FileText size={28} />
        </span>
        <div class="flex flex-col gap-1">
          <PageTitle documentTitle={t('privacy.title')}>{t('privacy.title')}</PageTitle>
          <p class="text-base text-muted">
            El Gobierno de Zapopan, a través del C5, es responsable de los datos personales que se
            recaban en el programa Sin Barreras.
          </p>
        </div>
      </div>

      <p class="flex items-start gap-3 rounded-card bg-success-soft p-4 text-base font-bold text-fg">
        <HeartPulse size={22} aria-hidden="true" class="mt-0.5 shrink-0 text-success" />
        Pedir ayuda nunca depende de leer este aviso: puedes hacerlo en cualquier momento.
      </p>

      <Block icon={<Database size={20} />} tint="sb-tint-1" title="¿Qué datos guardamos?">
        <ul class="flex list-disc flex-col gap-1 pl-6">
          <li>Los mensajes que envías y recibes, y las respuestas rápidas que tocas.</li>
          <li>
            Tu ubicación: la del punto donde escaneaste el código y, si lo permites, la de tu
            teléfono.
          </li>
          <li>
            Datos técnicos: dirección IP, tipo de navegador, idioma y ajustes de accesibilidad.
          </li>
          <li>Datos de la videollamada (no el video, que guarda el sistema de videollamadas).</li>
        </ul>
      </Block>

      <Block icon={<Lock size={20} />} tint="sb-tint-3" title="¿Para qué?">
        <p>
          Para atender tu emergencia y como respaldo legal y operativo de la atención. Algunos de
          estos datos son sensibles; los guardamos cifrados y solo personal autorizado puede
          consultarlos, y cada consulta queda registrada.
        </p>
      </Block>

      <Block icon={<Scale size={20} />} tint="sb-tint-4" title="Tus derechos">
        <p>
          Puedes ejercer tus derechos de acceso, rectificación, cancelación y oposición (ARCO) ante
          la Unidad de Transparencia del Gobierno de Zapopan.
        </p>
      </Block>

      <a
        href="/"
        class="inline-flex min-h-touch items-center justify-center rounded-full bg-primary px-6 font-extrabold text-on-primary shadow-card"
      >
        {t('app.back')}
      </a>
    </article>
  );
}
