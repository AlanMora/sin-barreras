import { describe, expect, it } from 'vitest';
import { parseEntry } from './entry';

describe('QR de entrada', () => {
  it('lee punto, llave y firma', () => {
    expect(parseEntry('?t=tot-0142&k=1&s=abc_DEF-1')).toEqual({
      point_id: 'TOT-0142',
      key_id: '1',
      signature: 'abc_DEF-1',
    });
  });
  it('ignora QR incompletos o con datos inválidos', () => {
    expect(parseEntry('')).toBeNull();
    expect(parseEntry('?t=TOT-0142')).toBeNull();
    expect(parseEntry('?t=<script>&s=x')).toBeNull();
    expect(parseEntry('?t=TOT-1&s=%00')).toBeNull();
  });
  it('una llave inválida no invalida el QR', () => {
    expect(parseEntry('?t=TOT-1&k=!!&s=x')).toEqual({ point_id: 'TOT-1', signature: 'x' });
  });
});
