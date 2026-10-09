/**
 * Ubicación del ciudadano.
 *
 * - El punto del QR ya viaja en el `hello`: es la ubicación de respaldo inmediata.
 * - El GPS se pide SOLO cuando el ciudadano pide ayuda (no antes) y se envía en
 *   cuanto llega; después se actualiza si la persona se mueve o pasa el tiempo.
 * - Si niegan el permiso o no hay GPS, todo sigue funcionando.
 */
import { signal } from '@preact/signals';

export type LocationStatus = 'idle' | 'requesting' | 'sharing' | 'denied' | 'unavailable';
export const locationStatus = signal<LocationStatus>('idle');

export interface Fix {
  latitude: number;
  longitude: number;
  accuracy: number;
  timestamp: number;
}

export interface LocationSink {
  send(fix: Fix): void;
  denied(): void;
  granted(): void;
}

/** Envía una actualización si pasaron 30 s o la persona se movió 50 m. */
const MIN_INTERVAL_MS = 30_000;
const MIN_DISTANCE_M = 50;

export function distanceMeters(a: Fix, b: Fix): number {
  const rad = (d: number) => (d * Math.PI) / 180;
  const dLat = rad(b.latitude - a.latitude);
  const dLon = rad(b.longitude - a.longitude);
  const h =
    Math.sin(dLat / 2) ** 2 +
    Math.cos(rad(a.latitude)) * Math.cos(rad(b.latitude)) * Math.sin(dLon / 2) ** 2;
  return 2 * 6_371_000 * Math.asin(Math.sqrt(h));
}

export function shouldSend(last: Fix | null, next: Fix): boolean {
  if (!last) return true;
  return (
    next.timestamp - last.timestamp >= MIN_INTERVAL_MS ||
    distanceMeters(last, next) >= MIN_DISTANCE_M
  );
}

let watchId: number | null = null;
let lastSent: Fix | null = null;

export function startLocationSharing(
  sink: LocationSink,
  geo: Geolocation | undefined = navigator.geolocation,
): void {
  if (watchId !== null || locationStatus.value === 'denied') return;
  if (!geo) {
    locationStatus.value = 'unavailable';
    return;
  }
  locationStatus.value = 'requesting';
  const onFix = (position: GeolocationPosition) => {
    if (locationStatus.value !== 'sharing') {
      locationStatus.value = 'sharing';
      sink.granted();
    }
    const fix: Fix = {
      latitude: position.coords.latitude,
      longitude: position.coords.longitude,
      accuracy: position.coords.accuracy,
      timestamp: position.timestamp,
    };
    if (shouldSend(lastSent, fix)) {
      lastSent = fix;
      sink.send(fix);
    }
  };
  const onError = (error: GeolocationPositionError) => {
    if (error.code === error.PERMISSION_DENIED) {
      locationStatus.value = 'denied';
      sink.denied();
      stopLocationSharing(geo);
    } else if (locationStatus.value === 'requesting') {
      // Sin señal todavía: se sigue intentando con watchPosition.
      locationStatus.value = 'unavailable';
    }
  };
  watchId = geo.watchPosition(onFix, onError, {
    enableHighAccuracy: true,
    maximumAge: 15_000,
    timeout: 20_000,
  });
}

export function stopLocationSharing(geo: Geolocation | undefined = navigator.geolocation): void {
  if (watchId !== null && geo) geo.clearWatch(watchId);
  watchId = null;
}
