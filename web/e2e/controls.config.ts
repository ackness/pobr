import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: '.', testMatch: ['controls.check.ts', 'ranges.check.ts'], workers: 1, timeout: 30_000,
  use: { baseURL: 'http://127.0.0.1:4177', screenshot: 'only-on-failure',
    launchOptions: { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH } },
  webServer: { command: 'pnpm dev --host 127.0.0.1 --port 4177 --strictPort', url: 'http://127.0.0.1:4177', reuseExistingServer: true },
});
