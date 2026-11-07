import { defineConfig } from '@playwright/test';
import { e2eServer } from './server';

const { port, baseURL } = e2eServer(4177);
export default defineConfig({
  testDir: '.', testMatch: ['controls.check.ts', 'ranges.check.ts'], workers: 1, timeout: 30_000,
  use: { baseURL, screenshot: 'only-on-failure',
    launchOptions: { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH } },
  webServer: { command: `pnpm dev --host 127.0.0.1 --port ${port} --strictPort`, url: baseURL, reuseExistingServer: false },
});
