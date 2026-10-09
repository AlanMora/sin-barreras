/**
 * Punto de entrada por QR: `/?t=TOT-0142&k=1&s=<firma>`.
 * Se lee una vez al abrir, se conserva en la pestaña y se borra de la barra de
 * direcciones (la firma no debe quedar a la vista ni en el historial).
 */
import type { QrEntry } from '../protocol/generated';

const KEY = 'sb.entry';
const POINT = /^[A-Z]{2,5}-[A-Z0-9]{1,12}$/;
const SIGNATURE = /^[A-Za-z0-9_-]{1,128}$/;

export function parseEntry(search: string): QrEntry | null {
  const params = new URLSearchParams(search);
  const point = params.get('t')?.trim().toUpperCase();
  const signature = params.get('s')?.trim();
  if (!point || !POINT.test(point) || !signature || !SIGNATURE.test(signature)) return null;
  const keyId = params.get('k')?.trim();
  return {
    point_id: point,
    signature,
    ...(keyId && /^[A-Za-z0-9]{1,16}$/.test(keyId) ? { key_id: keyId } : {}),
  };
}

export function captureEntry(): QrEntry | null {
  const fromUrl = parseEntry(location.search);
  if (fromUrl) {
    try {
      sessionStorage.setItem(KEY, JSON.stringify(fromUrl));
    } catch {
      // sin almacenamiento: vale para esta carga
    }
    history.replaceState(history.state, '', location.pathname);
    return fromUrl;
  }
  try {
    const saved = sessionStorage.getItem(KEY);
    return saved ? (JSON.parse(saved) as QrEntry) : null;
  } catch {
    return null;
  }
}
