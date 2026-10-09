import { FALLBACK_PHONE, formatPhone } from '../config';

/** Pantalla provisional de la Fase 0; se reemplaza en la Fase 3. */
export function Home() {
  return (
    <main class="mx-auto flex min-h-dvh max-w-xl flex-col justify-center gap-6 p-6">
      <h1 class="font-display text-4xl font-extrabold text-navy-900">Sin Barreras</h1>
      <p class="text-lg">Programa de Atención Incluyente de Emergencias del C5 de Zapopan.</p>
      <a
        href={`tel:${FALLBACK_PHONE}`}
        class="inline-flex min-h-touch items-center justify-center rounded-button bg-orange-500 px-6 text-lg font-bold text-navy-900"
      >
        Llamar al {formatPhone(FALLBACK_PHONE)}
      </a>
    </main>
  );
}
