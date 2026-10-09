/**
 * i18n mínima y tipada (ADR-0018): diccionarios planos por idioma, interpolación
 * `{nombre}` y el idioma activo como signal. Agregar un idioma = un archivo con
 * el tipo `Record<MessageKey, string>` (el compilador exige todas las claves).
 */
import { computed, signal } from '@preact/signals';
import { en } from './en';
import { es, type MessageKey } from './es';

export type { MessageKey };

export const LOCALES = {
  'es-MX': { label: 'Español', messages: es as Record<MessageKey, string> },
  en: { label: 'English', messages: en },
} as const;

export type Locale = keyof typeof LOCALES;

export function isLocale(value: string): value is Locale {
  return value in LOCALES;
}

/** Idioma inicial: el guardado, si no el del navegador, si no español. */
export function detectLocale(saved: string | null, navigatorLanguages: readonly string[]): Locale {
  if (saved && isLocale(saved)) return saved;
  for (const lang of navigatorLanguages) {
    if (lang.toLowerCase().startsWith('es')) return 'es-MX';
    if (lang.toLowerCase().startsWith('en')) return 'en';
  }
  return 'es-MX';
}

export const locale = signal<Locale>('es-MX');
const messages = computed(() => LOCALES[locale.value].messages);

export function t(key: MessageKey, params?: Record<string, string | number>): string {
  const template = messages.value[key] ?? es[key];
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in params ? String(params[name]) : match,
  );
}
