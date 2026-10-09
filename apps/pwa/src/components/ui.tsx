/** Piezas de interfaz accesibles hechas a mano (sin librerías de componentes). */
import type { ButtonHTMLAttributes, ComponentChildren } from 'preact';
import { useEffect, useRef } from 'preact/hooks';

/** Texto solo para lectores de pantalla. */
export function SrOnly({ children }: { children: ComponentChildren }) {
  return <span class="sr-only">{children}</span>;
}

/**
 * Última ruta en la que se movió el foco. En la carga inicial es `null`: no se
 * mueve el foco (el lector de pantalla empieza arriba y el primer Tab llega a
 * "Saltar al contenido"). Solo se mueve cuando cambia la ruta.
 */
let lastPath: string | null = null;

/**
 * Título de página: al NAVEGAR a otra pantalla recibe el foco para que el
 * lector de pantalla la anuncie, y actualiza el título del documento.
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
    const path = location.pathname + location.search;
    const navigated = lastPath !== null && lastPath !== path;
    lastPath = path;
    if (navigated) ref.current?.focus({ preventScroll: false });
  }, [documentTitle]);
  return (
    <h1
      ref={ref}
      tabIndex={-1}
      class="font-display text-[1.75rem] font-black leading-tight tracking-tight text-fg outline-none sm:text-4xl"
    >
      {children}
    </h1>
  );
}

type Variant = 'primary' | 'accent' | 'ghost' | 'danger';

const VARIANTS: Record<Variant, string> = {
  primary: 'bg-primary text-on-primary shadow-card',
  accent: 'bg-accent text-on-accent shadow-card',
  ghost: 'bg-surface text-fg border-2 border-line',
  danger: 'bg-danger text-bg',
};

const BASE =
  'inline-flex min-h-touch min-w-touch items-center justify-center gap-2 rounded-button px-5 py-2 text-lg font-extrabold transition-[transform,filter] hover:brightness-110 active:scale-[0.98] disabled:opacity-60';

export function Button({
  variant = 'primary' as Variant,
  class: extra = '',
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  return <button type="button" {...props} class={`${BASE} ${VARIANTS[variant]} ${extra}`} />;
}

const TONES = {
  info: { box: 'border-line-soft bg-surface', icon: 'sb-tint-2' },
  warning: { box: 'border-danger bg-danger-soft', icon: 'bg-danger text-bg' },
  success: { box: 'border-success bg-success-soft', icon: 'bg-success text-bg' },
} as const;

/** Tarjeta de aviso con ícono. `live` la convierte en región de estado. */
export function Notice({
  tone = 'info',
  icon,
  children,
  live = false,
}: {
  tone?: keyof typeof TONES;
  icon?: ComponentChildren;
  children: ComponentChildren;
  live?: boolean;
}) {
  const style = TONES[tone];
  return (
    <div
      {...(live ? { role: 'status' } : {})}
      class={`sb-rise flex items-start gap-3 rounded-card border-2 p-4 text-base shadow-card ${style.box}`}
    >
      {icon ? (
        <span
          aria-hidden="true"
          class={`grid h-10 w-10 shrink-0 place-items-center rounded-xl ${style.icon}`}
        >
          {icon}
        </span>
      ) : null}
      <div class="min-w-0 flex-1 self-center">{children}</div>
    </div>
  );
}

/** Logotipo: cruz naranja (ayuda) sobre azul marino. Decorativo. */
export function BrandMark({ size = 'md' }: { size?: 'sm' | 'md' | 'lg' }) {
  const box =
    size === 'lg'
      ? 'h-14 w-14 rounded-2xl'
      : size === 'sm'
        ? 'h-9 w-9 rounded-xl'
        : 'h-11 w-11 rounded-xl';
  const bar = size === 'lg' ? 'h-7 w-2' : size === 'sm' ? 'h-4 w-1.5' : 'h-5 w-1.5';
  const cross = size === 'lg' ? 'h-2 w-7' : size === 'sm' ? 'h-1.5 w-4' : 'h-1.5 w-5';
  return (
    <span aria-hidden="true" class={`relative grid shrink-0 place-items-center bg-primary ${box}`}>
      <span class={`block rounded-full bg-mark ${bar}`} />
      <span class={`absolute block rounded-full bg-mark ${cross}`} />
    </span>
  );
}

/** Encabezado de sección dentro de una página. */
export function SectionTitle({ id, children }: { id?: string; children: ComponentChildren }) {
  return (
    <h2 id={id} class="font-display text-lg font-extrabold text-fg">
      {children}
    </h2>
  );
}
