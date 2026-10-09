import { describe, expect, it } from 'vitest';
import { formatPhone, normalizePhone } from './config';

describe('normalizePhone', () => {
  it('quita espacios y guiones conservando el +', () => {
    expect(normalizePhone(' +52 33-2296-7625 ')).toBe('+523322967625');
  });

  it('acepta números sin lada internacional', () => {
    expect(normalizePhone('33 2296 7625')).toBe('3322967625');
  });
});

describe('formatPhone', () => {
  it('formatea números de 10 dígitos de México', () => {
    expect(formatPhone('+523322967625')).toBe('33 2296 7625');
    expect(formatPhone('3322967625')).toBe('33 2296 7625');
  });

  it('deja intactos los formatos desconocidos', () => {
    expect(formatPhone('911')).toBe('911');
  });
});
