import { expect, test } from '@playwright/test';
import { inChat, myMessage, sendChat, waitConnected } from './helpers';

test('pedir ayuda por chat: enviado → entregado → leído y respuesta del operador', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByRole('link', { name: /Chat de texto/ }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Chat con el C5');
  await waitConnected(page);
  await sendChat(page, 'Me caí y no puedo caminar');
  await expect(myMessage(page, 'Me caí y no puedo caminar')).toContainText(/Entregado|Leído/);
  await expect(page.getByRole('main').getByText('Te estamos atendiendo.')).toBeVisible();
  await expect(inChat(page, /Te atiende el C5 Zapopan/)).toBeVisible();
  await expect(myMessage(page, 'Me caí y no puedo caminar')).toContainText('Leído');
});

test('la conversación se reconstruye al recargar la página', async ({ page }) => {
  await page.goto('/chat');
  await waitConnected(page);
  await sendChat(page, 'Mensaje antes de recargar');
  await expect(inChat(page, /Te atiende el C5 Zapopan/)).toBeVisible();
  await page.reload();
  await expect(myMessage(page, 'Mensaje antes de recargar')).toBeVisible();
  await expect(inChat(page, /Te atiende el C5 Zapopan/)).toBeVisible();
});

test('videollamada LSM: la PWA solicita y recibe los datos de la sala', async ({ page }) => {
  await page.goto('/llamada?m=video');
  await expect(page.getByText('La llamada está lista.')).toBeVisible();
  const join = page.getByRole('link', { name: 'Entrar a la llamada' });
  await expect(join).toHaveAttribute('href', /^https:\/\/video\.mock\.invalid\/rooms\//);
});

test('modo silencioso: respuesta rápida y salida rápida a pantalla neutral', async ({ page }) => {
  await page.goto('/silencioso');
  await waitConnected(page);
  await expect(page.locator('html')).toHaveAttribute('data-mode', 'silent');
  await page.getByRole('button', { name: 'Estoy en peligro' }).click();
  await expect(myMessage(page, 'Estoy en peligro')).toContainText(/Entregado|Leído/);
  await page.getByRole('button', { name: /Salida rápida/ }).click();
  await expect(page).toHaveURL(/\/clima$/);
  await expect(page.getByRole('heading', { name: 'Clima en Zapopan' })).toBeVisible();
  await expect(page.getByText(/C5|Sin Barreras|emergencia/i)).toHaveCount(0);
  // "Atrás" no regresa a la pantalla de emergencia.
  await page.goBack();
  await expect(page).not.toHaveURL(/\/silencioso$/);
});

test('QR firmado inválido: la ayuda NO se bloquea', async ({ page }) => {
  await page.goto('/?t=TOT-0142&k=1&s=firmaFalsaQueNoVerifica');
  await expect(page).toHaveURL(/\/$/); // la firma se quita de la barra de direcciones
  await page.getByRole('link', { name: /Chat de texto/ }).click();
  await waitConnected(page);
  await sendChat(page, 'Ayuda desde un QR dañado');
  await expect(myMessage(page, 'Ayuda desde un QR dañado')).toContainText(/Entregado|Leído/);
});
