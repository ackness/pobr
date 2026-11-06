import { defineConfig } from '@playwright/test';

// Isolated component workflow: no production app shell or WASM artifacts needed.
// Traces only in CI or with POBR_E2E_TRACE=1, as in playwright.config.ts.
export default defineConfig({
  outputDir: '../test-results/calcs-config',
  testDir: '.', testMatch: 'calcs-config.spec.ts', timeout: 30_000,
  fullyParallel: true, workers: 2,
  use: { baseURL: 'http://localhost:4184', screenshot: 'only-on-failure', trace: process.env.CI || process.env.POBR_E2E_TRACE === '1' ? 'retain-on-failure' : 'off' },
  webServer: { command: 'VITE_POBR_BACKEND=mock pnpm dev --config e2e/calcs-config.vite.config.ts --port 4184 --strictPort',
    url: 'http://localhost:4184', reuseExistingServer: false, timeout: 30_000 },
});
