import { expect, type Page } from '@playwright/test';

/** Espera a que la app esté conectada con el C5 (no hay aviso de conexión). */
export async function waitConnected(page: Page): Promise<void> {
  await expect(page.getByText(/Conectando con el C5|Reconectando|Sin internet/)).toHaveCount(0);
}

export async function sendChat(page: Page, text: string): Promise<void> {
  await page.getByLabel('Tu mensaje').fill(text);
  await page.getByRole('button', { name: 'Enviar' }).click();
}

/** Burbuja de un mensaje propio. */
export function myMessage(page: Page, text: string) {
  return page.getByRole('listitem').filter({ hasText: text });
}

/** Mensaje dentro de la conversación (no en la región para lectores de pantalla). */
export function inChat(page: Page, text: string | RegExp) {
  return page
    .getByRole('list', { name: 'Conversación' })
    .getByRole('listitem')
    .filter({ hasText: text });
}
