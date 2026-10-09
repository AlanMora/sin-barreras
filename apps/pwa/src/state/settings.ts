/**
 * Ajustes de accesibilidad e idioma: persistidos en el dispositivo y aplicados
 * al documento (tamaño de letra en el elemento raíz para que todo escale en
 * `rem`, alto contraste y modo silencioso como atributos de datos).
 */
import { effect, signal } from '@preact/signals';
import { detectLocale, type Locale, locale } from '../i18n';
import type { AccessibilitySettings } from '../protocol/generated';

export const FONT_SCALES = [1, 1.25, 1.5, 2] as const;
export type FontScale = (typeof FONT_SCALES)[number];

export const fontScale = signal<FontScale>(1);
export const highContrast = signal(false);
/** Activo mientras el ciudadano está en el modo silencioso. */
export const silentMode = signal(false);

const KEY = 'sb.settings';

interface Persisted {
  locale?: string;
  fontScale?: number;
  highContrast?: boolean;
}

function read(): Persisted {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? '{}') as Persisted;
  } catch {
    return {};
  }
}

export function prefersReducedMotion(): boolean {
  return typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
}

export function accessibilitySnapshot(): AccessibilitySettings {
  return {
    font_scale: fontScale.value,
    high_contrast: highContrast.value,
    reduced_motion: prefersReducedMotion(),
    silent_mode: silentMode.value,
  };
}

export function initSettings(): void {
  const saved = read();
  locale.value = detectLocale(saved.locale ?? null, navigator.languages ?? [navigator.language]);
  if (FONT_SCALES.includes(saved.fontScale as FontScale))
    fontScale.value = saved.fontScale as FontScale;
  highContrast.value = saved.highContrast === true;

  effect(() => {
    const root = document.documentElement;
    root.lang = locale.value;
    root.style.fontSize = `${fontScale.value * 100}%`;
    root.dataset.contrast = highContrast.value ? 'high' : 'normal';
    root.dataset.mode = silentMode.value ? 'silent' : 'normal';
    const data: Persisted = {
      locale: locale.value,
      fontScale: fontScale.value,
      highContrast: highContrast.value,
    };
    try {
      localStorage.setItem(KEY, JSON.stringify(data));
    } catch {
      // Sin almacenamiento: los ajustes duran lo que dure la página.
    }
  });
}

export function setLocale(next: Locale): void {
  locale.value = next;
}
