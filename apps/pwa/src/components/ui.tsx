/** Piezas de interfaz accesibles hechas a mano (sin librerías de componentes). */
import type { ButtonHTMLAttributes, ComponentChildren } from 'preact';
import { useEffect, useRef } from 'preact/hooks';

/** Texto solo para lectores de pantalla. */
export function SrOnly({ children }: { children: ComponentChildren }) {
  return <span class="sr-only">{children}</span>;
}

/**
 * Título de página: recibe el foco al montarse para que el lector de pantalla
 * anuncie la pantalla nueva, y actualiza el título del documento.
 */
export function PageTitle({
  children,
  documentTitle,
}: {
  children: ComponentChildren;
  documentTitle: string;
}) {
  const ref = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    document.title = documentTitle;
    ref.current?.focus({ preventScroll: false });
  }, [documentTitle]);
  return (
    <h1
      ref={ref}
      tabIndex={-1}
      class="font-display text-3xl font-extrabold leading-tight text-fg outline-none sm:text-4xl"
    >
      {children}
    </h1>
  );
}

type Variant = 'primary' | 'accent' | 'ghost' | 'danger';

const VARIANTS: Record<Variant, string> = {
  primary: 'bg-primary text-on-primary',
  accent: 'bg-accent text-on-accent',
  ghost: 'bg-transparent text-fg border-2 border-line',
  danger: 'bg-danger text-bg',
};

const BASE =
  'inline-flex min-h-touch min-w-touch items-center justify-center gap-2 rounded-button px-5 py-2 text-lg font-extrabold transition-transform active:scale-[0.98] disabled:opacity-60';

export function Button({
  variant = 'primary' as Variant,
  class: extra = '',
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  return <button type="button" {...props} class={`${BASE} ${VARIANTS[variant]} ${extra}`} />;
}

/** Tarjeta de aviso con ícono. `live` la convierte en región de estado. */
export function Notice({
  tone = 'info',
  icon,
  children,
  live = false,
}: {
  tone?: 'info' | 'warning' | 'success';
  icon?: ComponentChildren;
  children: ComponentChildren;
  live?: boolean;
}) {
  const border =
    tone === 'warning' ? 'border-danger' : tone === 'success' ? 'border-success' : 'border-line';
  return (
    <div
      {...(live ? { role: 'status' } : {})}
      class={`flex items-start gap-3 rounded-card border-2 ${border} bg-surface p-4 text-base`}
    >
      {icon ? <span class="mt-0.5 shrink-0">{icon}</span> : null}
      <div class="min-w-0 flex-1">{children}</div>
    </div>
  );
}
