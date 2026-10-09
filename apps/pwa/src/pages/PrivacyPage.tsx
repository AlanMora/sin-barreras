import { useEffect } from 'preact/hooks';
import { PageTitle } from '../components/ui';
import { t } from '../i18n';
import { clientEvent } from '../services';

/**
 * Aviso de privacidad integral. BORRADOR técnico: el texto definitivo lo
 * aprueba el área jurídica del municipio (LFPDPPP y normativa de sujetos
 * obligados). Ver docs/aviso-de-privacidad.md.
 */
export function PrivacyPage() {
  useEffect(() => clientEvent('privacy_notice_opened'), []);
  return (
    <article class="flex flex-col gap-4 text-lg leading-relaxed">
      <PageTitle documentTitle={t('privacy.title')}>{t('privacy.title')}</PageTitle>
      <p>
        El Gobierno de Zapopan, a través del C5, es responsable de los datos personales que se
        recaban en el programa Sin Barreras.
      </p>
      <h2 class="font-display text-xl font-extrabold">¿Qué datos guardamos?</h2>
      <ul class="list-disc pl-6">
        <li>Los mensajes que envías y recibes, y las respuestas rápidas que tocas.</li>
        <li>
          Tu ubicación: la del punto donde escaneaste el código y, si lo permites, la de tu
          teléfono.
        </li>
        <li>Datos técnicos: dirección IP, tipo de navegador, idioma y ajustes de accesibilidad.</li>
        <li>Datos de la videollamada (no el video, que guarda el sistema de videollamadas).</li>
      </ul>
      <h2 class="font-display text-xl font-extrabold">¿Para qué?</h2>
      <p>
        Para atender tu emergencia y como respaldo legal y operativo de la atención. Algunos de
        estos datos son sensibles; los guardamos cifrados y solo personal autorizado puede
        consultarlos, y cada consulta queda registrada.
      </p>
      <h2 class="font-display text-xl font-extrabold">Tus derechos</h2>
      <p>
        Puedes ejercer tus derechos de acceso, rectificación, cancelación y oposición (ARCO) ante la
        Unidad de Transparencia del Gobierno de Zapopan.
      </p>
      <p class="text-base text-muted">
        Pedir ayuda nunca depende de leer este aviso: puedes hacerlo en cualquier momento.
      </p>
      <a href="/" class="inline-flex min-h-touch items-center font-bold underline">
        {t('app.back')}
      </a>
    </article>
  );
}
