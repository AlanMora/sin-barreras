import preact from '@preact/preset-vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';
import { VitePWA } from 'vite-plugin-pwa';

export default defineConfig({
  plugins: [
    preact(),
    tailwindcss(),
    VitePWA({
      // `prompt` evita que una actualización recargue la página a media
      // emergencia: el usuario decide cuándo aplicar la nueva versión.
      registerType: 'prompt',
      injectRegister: false,
      manifest: {
        name: 'Sin Barreras — Atención Incluyente de Emergencias',
        short_name: 'Sin Barreras',
        description: 'Pide ayuda de emergencia al C5 de Zapopan sin hacer una llamada.',
        lang: 'es-MX',
        start_url: '/',
        display: 'standalone',
        background_color: '#ffffff',
        theme_color: '#0b2545',
        icons: [],
      },
      workbox: {
        // Solo se precachea el shell estático. El WebSocket y la API nunca
        // pasan por el caché del service worker.
        globPatterns: ['**/*.{js,css,html,svg,woff2}'],
        navigateFallback: '/index.html',
        navigateFallbackDenylist: [/^\/api\//, /^\/ws/],
      },
    }),
  ],
  build: {
    target: 'es2022',
    manifest: true,
    // Mapas para depurar, sin referencia pública en el JS (nginx no los sirve).
    sourcemap: 'hidden',
  },
});
