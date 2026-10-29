import { defineConfig } from '@playwright/test';

// POBR_E2E_PORT lets a second run use its own preview server and port next to the default one.
const custom = process.env.POBR_E2E_PORT;
const port = Number(custom ?? 4173);
const baseURL = `http://localhost:${port}`;

export default defineConfig({
  testDir: 'e2e',
  testIgnore: ['calcs-config.spec.ts', 'item-display.spec.ts'], // Isolated fixtures have their own server/config.
  timeout: 120_000,
  use: {
    baseURL,
    // The app starts in Chinese on a browser that has never saved a preference; a device that used the
    // first interface keeps the language it chose there (`pobr-lang`), which is what the specs rely on.
    storageState: { cookies: [], origins: [{ origin: baseURL, localStorage: [{ name: 'pobr-lang', value: 'en-US' }] }] },
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
  },
  webServer: {
    // preview 用已构建的 dist（含 wasm + public/data）；先 pnpm build + sync-data。
    command: `pnpm preview --port ${port} --strictPort`,
    url: baseURL,
    reuseExistingServer: !custom,
    timeout: 60_000,
  },
});
