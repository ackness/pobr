import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: 'e2e',
  timeout: 120_000,
  use: {
    baseURL: 'http://localhost:4173',
    screenshot: 'only-on-failure',
  },
  webServer: {
    // preview 用已构建的 dist（含 wasm + public/data）；先 npm run build + sync-data。
    command: 'npm run preview -- --port 4173 --strictPort',
    url: 'http://localhost:4173',
    reuseExistingServer: true,
    timeout: 60_000,
  },
});
