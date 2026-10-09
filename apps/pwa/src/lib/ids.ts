/** Identificadores y contexto de trazas (sin dependencias). */

function randomHex(bytes: number): string {
  const buf = new Uint8Array(bytes);
  crypto.getRandomValues(buf);
  return Array.from(buf, (b) => b.toString(16).padStart(2, '0')).join('');
}

/** UUID v4. Usa `crypto.randomUUID` si existe (contexto seguro). */
export function uuid(): string {
  if (typeof crypto.randomUUID === 'function') return crypto.randomUUID();
  const h = randomHex(16).split('');
  h[12] = '4';
  h[16] = ((Number.parseInt(h[16] ?? '0', 16) & 0x3) | 0x8).toString(16);
  const s = h.join('');
  return `${s.slice(0, 8)}-${s.slice(8, 12)}-${s.slice(12, 16)}-${s.slice(16, 20)}-${s.slice(20)}`;
}

/**
 * Contexto W3C `traceparent` para correlacionar una acción del ciudadano con
 * las trazas del BFF y del C5 (ADR-0008: sin el SDK de OpenTelemetry).
 */
export function traceparent(): string {
  return `00-${randomHex(16)}-${randomHex(8)}-01`;
}

/** Marca de tiempo RFC 3339 en UTC con milisegundos. */
export function nowIso(): string {
  return new Date().toISOString();
}
