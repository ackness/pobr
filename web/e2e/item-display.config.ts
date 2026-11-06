import { defineConfig } from '@playwright/test';

// Traces only in CI or with POBR_E2E_TRACE=1, as in playwright.config.ts.
export default defineConfig({
  outputDir: '../test-results/item-display',
  testDir: '.', testMatch: ['item-display.spec.ts', 'global-display.spec.ts'], timeout: 60_000, workers: 1,
  use: { baseURL: 'http://localhost:4186', screenshot: 'only-on-failure', trace: process.env.CI || process.env.POBR_E2E_TRACE === '1' ? 'retain-on-failure' : 'off' },
  webServer: {
    command: 'VITE_POBR_BACKEND=mock pnpm dev --port 4186 --strictPort',
    url: 'http://localhost:4186', reuseExistingServer: false, timeout: 30_000,
  },
});
