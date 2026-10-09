import { render } from 'preact';
import { App } from './app';
import { startServices } from './services';
import { initSettings } from './state/settings';
import './styles.css';

initSettings();
const root = document.getElementById('app');
if (root) render(<App />, root);
// La conexión arranca de inmediato: cuando el ciudadano pida ayuda, ya está lista.
void startServices();

// Service worker: actualización controlada (nunca recarga a media emergencia).
if ('serviceWorker' in navigator && import.meta.env.PROD) {
  void import('virtual:pwa-register').then(({ registerSW }) => registerSW({ immediate: true }));
}
