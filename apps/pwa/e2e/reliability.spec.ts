import { expect, test } from '@playwright/test';
import { inChat, myMessage, sendChat, waitConnected } from './helpers';

test('sin red: el mensaje queda en cola y se entrega al volver la conexión', async ({
  page,
  context,
}) => {
  await page.goto('/chat');
  await waitConnected(page);
  await context.setOffline(true);
  await page.evaluate(() => window.dispatchEvent(new Event('offline')));
  await expect(page.getByText(/Sin internet|Reconectando/)).toBeVisible();
  // El respaldo telefónico se ofrece mientras no hay conexión.
  await expect(page.getByRole('link', { name: /Llamar al 33 2296 7625/ }).first()).toBeVisible();
  await sendChat(page, 'Escrito sin internet');
  await expect(myMessage(page, 'Escrito sin internet')).toContainText('En cola');

  await context.setOffline(false);
  await page.evaluate(() => window.dispatchEvent(new Event('online')));
  await expect(myMessage(page, 'Escrito sin internet')).toContainText(/Entregado|Leído/, {
    timeout: 30_000,
  });
});

test('recargar sin red: la app abre desde el service worker y el mensaje no se pierde', async ({
  page,
  context,
}) => {
  await page.goto('/chat');
  await waitConnected(page);
  await page.evaluate(async () => {
    await navigator.serviceWorker.ready;
  });
  await page.reload(); // ahora controlada por el service worker
  await waitConnected(page);

  await context.setOffline(true);
  await page.evaluate(() => window.dispatchEvent(new Event('offline')));
  await sendChat(page, 'Antes de que iOS mate la pestaña');
  await expect(myMessage(page, 'Antes de que iOS mate la pestaña')).toContainText('En cola');
  await page.reload();
  // Sin red: el shell sale del caché y la cola persistente sigue ahí.
  await expect(myMessage(page, 'Antes de que iOS mate la pestaña')).toContainText('En cola');

  await context.setOffline(false);
  await page.evaluate(() => window.dispatchEvent(new Event('online')));
  await expect(myMessage(page, 'Antes de que iOS mate la pestaña')).toContainText(
    /Entregado|Leído/,
    {
      timeout: 30_000,
    },
  );
});

test('red que bloquea WebSocket: la conversación sigue por HTTP', async ({ page }) => {
  // Simula un proxy que corta todo WebSocket.
  await page.routeWebSocket(/\/ws$/, (ws) => ws.close({ code: 1006 }));
  await page.goto('/chat');
  await sendChat(page, 'Sin WebSocket');
  await expect(myMessage(page, 'Sin WebSocket')).toContainText(/Entregado|Leído/, {
    timeout: 45_000,
  });
  await expect(inChat(page, /Te atiende el C5 Zapopan/)).toBeVisible({ timeout: 30_000 });
});

test('volver del segundo plano (iOS): reconecta y recupera lo que llegó', async ({ page }) => {
  await page.goto('/chat');
  await waitConnected(page);
  await sendChat(page, 'Bloqueo el teléfono');
  await expect(inChat(page, /Te atiende el C5 Zapopan/)).toBeVisible();
  await page.evaluate(() => {
    Object.defineProperty(document, 'visibilityState', { value: 'hidden', configurable: true });
    document.dispatchEvent(new Event('visibilitychange'));
    Object.defineProperty(document, 'visibilityState', { value: 'visible', configurable: true });
    document.dispatchEvent(new Event('visibilitychange'));
  });
  await expect(inChat(page, /Gracias, recibido/)).toBeVisible({ timeout: 20_000 });
});
