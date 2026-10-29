import { defineConfig } from '@playwright/test';

export default defineConfig({
  outputDir: '../test-results/item-display',
  testDir: '.', testMatch: ['item-display.spec.ts', 'global-display.spec.ts'], timeout: 60_000, workers: 1,
  use: { baseURL: 'http://localhost:4186', screenshot: 'only-on-failure', trace: 'retain-on-failure' },
  webServer: {
    command: 'VITE_POBR_BACKEND=mock pnpm dev --port 4186 --strictPort',
    url: 'http://localhost:4186', reuseExistingServer: false, timeout: 30_000,
  },
});
