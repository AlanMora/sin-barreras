import { defineConfig, devices } from '@playwright/test';

/**
 * E2E contra el sistema completo (Traefik → PWA + 2 réplicas del BFF → C5
 * simulado → registro). Levantar antes: `make up-apps`.
 */
export default defineConfig({
  testDir: './e2e',
  timeout: 60_000,
  expect: { timeout: 15_000 },
  fullyParallel: true,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [['list'], ['html', { open: 'never' }]] : 'list',
  use: {
    baseURL: process.env.E2E_BASE_URL ?? 'http://localhost:8000',
    locale: 'es-MX',
    trace: 'retain-on-failure',
    serviceWorkers: 'allow',
  },
  projects: [{ name: 'android', use: { ...devices['Pixel 7'] } }],
});
