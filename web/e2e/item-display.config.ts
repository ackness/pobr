import { defineConfig } from '@playwright/test';
import { e2eServer } from './server';

const { port, baseURL } = e2eServer(4186);
// Traces only in CI or with POBR_E2E_TRACE=1, as in playwright.config.ts.
export default defineConfig({
  outputDir: '../test-results/item-display',
  testDir: '.', testMatch: ['item-display.spec.ts', 'global-display.spec.ts'], timeout: 60_000, workers: 1,
  use: { baseURL, screenshot: 'only-on-failure', trace: process.env.CI || process.env.POBR_E2E_TRACE === '1' ? 'retain-on-failure' : 'off' },
  webServer: {
    command: `VITE_POBR_BACKEND=mock pnpm dev --host 127.0.0.1 --port ${port} --strictPort`,
    url: baseURL, reuseExistingServer: false, timeout: 30_000,
  },
});
