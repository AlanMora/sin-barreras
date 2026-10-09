import { expect, test } from '@playwright/test';
import { inChat, sendChat, waitConnected } from './helpers';

test('cabeceras de seguridad y CSP estricta en la PWA y el BFF', async ({ page, request }) => {
  const response = await page.goto('/');
  const headers = response?.headers() ?? {};
  const csp = headers['content-security-policy'] ?? '';
  expect(csp).toContain("default-src 'self'");
  expect(csp).toContain("script-src 'self'");
  expect(csp).not.toContain('unsafe-inline');
  expect(csp).not.toContain('unsafe-eval');
  expect(csp).toContain("frame-ancestors 'none'");
  expect(headers['x-content-type-options']).toBe('nosniff');
  expect(headers['referrer-policy']).toBe('no-referrer');

  const api = await request.post('/api/v1/hello', { data: { type: 'ping' } });
  expect(api.headers()['cache-control']).toBe('no-store');
  expect(api.headers()['x-content-type-options']).toBe('nosniff');
});

test('la app funciona completa sin ninguna violación de CSP', async ({ page }) => {
  const violations: string[] = [];
  await page.addInitScript(() => {
    document.addEventListener('securitypolicyviolation', (e) => {
      (window as unknown as { __csp: string[] }).__csp ??= [];
      (window as unknown as { __csp: string[] }).__csp.push(`${e.violatedDirective} ${e.blockedURI}`);
    });
  });
  page.on('console', (m) => {
    if (m.type() === 'error' && /Content Security Policy/i.test(m.text())) violations.push(m.text());
  });
  await page.goto('/');
  await page.getByRole('link', { name: /Chat de texto/ }).click();
  await waitConnected(page);
  await sendChat(page, 'Probando CSP');
  await expect(inChat(page, /Te atiende el C5 Zapopan/)).toBeVisible();
  await page.goto('/ajustes');
  await page.getByLabel('Muy grande').check();
  await page.getByLabel(/Alto contraste/).check();
  await page.goto('/silencioso');
  await waitConnected(page);
  const inPage = await page.evaluate(() => (window as unknown as { __csp?: string[] }).__csp ?? []);
  expect([...violations, ...inPage]).toEqual([]);
});

test('un origen ajeno no puede abrir el WebSocket ni usar el API', async ({ request }) => {
  const hello = await request.post('/api/v1/hello', {
    headers: { origin: 'https://sitio-malicioso.example' },
    data: { type: 'hello', client_msg_id: crypto.randomUUID(), protocol_version: 1, app_version: 'x', language: 'es' },
  });
  expect(hello.status()).toBe(403);
});
