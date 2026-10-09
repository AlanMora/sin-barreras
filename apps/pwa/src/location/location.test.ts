import { describe, expect, it } from 'vitest';
import { distanceMeters, type Fix, shouldSend } from './location';

const fix = (latitude: number, longitude: number, timestamp: number): Fix => ({
  latitude,
  longitude,
  accuracy: 10,
  timestamp,
});

describe('ubicación', () => {
  it('calcula distancias razonables', () => {
    // ~111 m por milésima de grado de latitud
    expect(distanceMeters(fix(20.72, -103.39, 0), fix(20.721, -103.39, 0))).toBeGreaterThan(100);
    expect(distanceMeters(fix(20.72, -103.39, 0), fix(20.721, -103.39, 0))).toBeLessThan(120);
  });

  it('envía la primera, luego solo si pasó tiempo o hubo movimiento', () => {
    const first = fix(20.72, -103.39, 0);
    expect(shouldSend(null, first)).toBe(true);
    expect(shouldSend(first, fix(20.72001, -103.39, 5_000))).toBe(false);
    expect(shouldSend(first, fix(20.72, -103.39, 31_000))).toBe(true);
    expect(shouldSend(first, fix(20.7206, -103.39, 5_000))).toBe(true);
  });
});
