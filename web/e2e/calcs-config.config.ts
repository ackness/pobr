import { defineConfig } from '@playwright/test';
import { e2eServer } from './server';

const { port, baseURL } = e2eServer(4184);
// Isolated component workflow: no production app shell or WASM artifacts needed.
// Traces only in CI or with POBR_E2E_TRACE=1, as in playwright.config.ts.
export default defineConfig({
  outputDir: '../test-results/calcs-config',
  testDir: '.', testMatch: 'calcs-config.spec.ts', timeout: 30_000,
  fullyParallel: true, workers: 2,
  use: { baseURL, screenshot: 'only-on-failure', trace: process.env.CI || process.env.POBR_E2E_TRACE === '1' ? 'retain-on-failure' : 'off' },
  webServer: { command: `VITE_POBR_BACKEND=mock pnpm dev --config e2e/calcs-config.vite.config.ts --host 127.0.0.1 --port ${port} --strictPort`,
    url: baseURL, reuseExistingServer: false, timeout: 30_000 },
});
