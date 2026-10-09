/**
 * Número de respaldo cuando no hay conexión con el BFF.
 * Se toma de `VITE_FALLBACK_PHONE` en build; el valor por defecto es el
 * número acordado para el programa.
 */
export const FALLBACK_PHONE: string = normalizePhone(
  import.meta.env.VITE_FALLBACK_PHONE ?? '+523322967625',
);

/** Deja solo dígitos y un `+` inicial (formato válido para `tel:` y `sms:`). */
export function normalizePhone(raw: string): string {
  const trimmed = raw.trim();
  const digits = trimmed.replace(/\D/g, '');
  return trimmed.startsWith('+') ? `+${digits}` : digits;
}

/** Formato legible para México: `33 2296 7625`. */
export function formatPhone(phone: string): string {
  const local = phone.replace(/^\+52/, '');
  const match = /^(\d{2})(\d{4})(\d{4})$/.exec(local);
  return match ? `${match[1]} ${match[2]} ${match[3]}` : phone;
}
