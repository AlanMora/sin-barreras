import AxeBuilder from '@axe-core/playwright';
import { expect, type Page, test } from '@playwright/test';
import { waitConnected } from './helpers';

/** Auditoría automática WCAG 2.1 A/AA con axe-core. */
async function audit(page: Page) {
  const results = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
    .analyze();
  const summary = results.violations.map(
    (v) => `${v.id}: ${v.help} → ${v.nodes.map((n) => n.target.join(' ')).join(', ')}`,
  );
  expect(summary, summary.join('\n')).toEqual([]);
}

const screens = [
  '/',
  '/chat',
  '/silencioso',
  '/llamada?m=video',
  '/idioma',
  '/ajustes',
  '/aviso-de-privacidad',
  '/clima',
];

for (const path of screens) {
  test(`WCAG 2.1 AA: ${path}`, async ({ page }) => {
    await page.goto(path);
    await waitConnected(page);
    await page.waitForTimeout(500);
    await audit(page);
  });
}

test('WCAG 2.1 AA con alto contraste y letra muy grande', async ({ page }) => {
  await page.goto('/ajustes');
  await page.getByLabel(/Alto contraste/).check();
  await page.getByLabel('Muy grande').check();
  for (const path of ['/', '/chat', '/silencioso']) {
    await page.goto(path);
    await waitConnected(page);
    await audit(page);
  }
});

test('WCAG 2.1 AA en inglés', async ({ page }) => {
  await page.goto('/ajustes');
  await page.getByLabel('English').check();
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  await page.goto('/');
  await expect(page.getByRole('heading', { level: 1 })).toHaveText(
    'How do you want to communicate?',
  );
  await audit(page);
});

test('teclado: saltar al contenido y elegir un canal sin ratón', async ({ page }) => {
  await page.goto('/');
  await page.keyboard.press('Tab');
  const skip = page.getByRole('link', { name: 'Saltar al contenido' });
  await expect(skip).toBeFocused();
  await page.keyboard.press('Enter');
  // Desde el contenido principal, el primer elemento enfocable es un canal.
  await page.keyboard.press('Tab');
  await expect(page.getByRole('link', { name: /Videollamada en Lengua de Señas/ })).toBeFocused();
  await page.keyboard.press('Tab');
  await page.keyboard.press('Enter');
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Chat con el C5');
  await expect(page.getByRole('heading', { level: 1 })).toBeFocused();
});

test('objetivos táctiles de al menos 48 px', async ({ page }) => {
  for (const path of ['/', '/chat', '/silencioso', '/ajustes']) {
    await page.goto(path);
    const small = await page.$$eval(
      'main a, main button, main input, main textarea, header a',
      (els) =>
        els
          .filter((el) => (el as HTMLElement).offsetParent !== null)
          .map((el) => {
            const target = el.closest('label') ?? el;
            const r = target.getBoundingClientRect();
            return {
              text: (el.textContent ?? (el as HTMLInputElement).name).trim().slice(0, 30),
              w: r.width,
              h: r.height,
            };
          })
          .filter((r) => r.w < 47.5 || r.h < 47.5),
    );
    expect(small, `${path}: ${JSON.stringify(small)}`).toEqual([]);
  }
});
