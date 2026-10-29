import { defineConfig } from '@playwright/test';

// Isolated component workflow: no production app shell or WASM artifacts needed.
export default defineConfig({
  outputDir: '../test-results/calcs-config',
  testDir: '.', testMatch: 'calcs-config.spec.ts', timeout: 30_000,
  fullyParallel: true, workers: 2,
  use: { baseURL: 'http://localhost:4184', screenshot: 'only-on-failure', trace: 'retain-on-failure' },
  webServer: { command: 'VITE_POBR_BACKEND=mock pnpm dev --config e2e/calcs-config.vite.config.ts --port 4184 --strictPort',
    url: 'http://localhost:4184', reuseExistingServer: false, timeout: 30_000 },
});
