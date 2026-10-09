import { describe, expect, it } from 'vitest';
import { en } from './en';
import { es } from './es';
import { detectLocale, locale, t } from './index';

describe('i18n', () => {
  it('inglés tiene exactamente las mismas claves que español', () => {
    expect(Object.keys(en).sort()).toEqual(Object.keys(es).sort());
  });

  it('ningún texto está vacío', () => {
    for (const dict of [es, en]) {
      for (const [key, value] of Object.entries(dict)) expect(value.trim(), key).not.toBe('');
    }
  });

  it('interpola parámetros', () => {
    locale.value = 'es-MX';
    expect(t('fallback.call', { phone: '33 2296 7625' })).toBe('Llamar al 33 2296 7625');
    locale.value = 'en';
    expect(t('incident.unit_en_route_eta', { minutes: 8 })).toContain('8 min');
    locale.value = 'es-MX';
  });

  it('detecta el idioma del navegador con español por defecto', () => {
    expect(detectLocale(null, ['en-US'])).toBe('en');
    expect(detectLocale(null, ['fr-FR'])).toBe('es-MX');
    expect(detectLocale('en', ['es-MX'])).toBe('en');
    expect(detectLocale('xx', ['es'])).toBe('es-MX');
  });
});
