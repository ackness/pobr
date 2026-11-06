import { defineConfig } from '@playwright/test';

// POBR_E2E_PORT lets a second run use its own preview server and port next to the default one.
const custom = process.env.POBR_E2E_PORT;
const port = Number(custom ?? 4173);
const baseURL = `http://localhost:${port}`;
// Tests tagged @quarantine or @extended (e2e/tags.ts) stay out of the default run; POBR_E2E_ALL=1 includes them.
const all = process.env.POBR_E2E_ALL === '1';
// Recording a trace costs every test CPU time. CI keeps traces of failures; locally POBR_E2E_TRACE=1 turns them on.
const traces = !!process.env.CI || process.env.POBR_E2E_TRACE === '1';

export default defineConfig({
  testDir: 'e2e',
  testIgnore: ['calcs-config.spec.ts', 'item-display.spec.ts', 'global-display.spec.ts'], // Isolated fixtures have their own server/config.
  grepInvert: all ? undefined : /@quarantine|@extended/,
  timeout: 120_000,
  // Each worker is a Chromium renderer running real WASM plus its GPU process (about two cores). CI keeps
  // Playwright's default (half the runner's cores); `--workers` overrides both.
  workers: process.env.CI ? undefined : 4,
  use: {
    baseURL,
    // The app starts in Chinese on a browser that has never saved a preference; a device that used the
    // first interface keeps the language it chose there (`pobr-lang`), which is what the specs rely on.
    storageState: { cookies: [], origins: [{ origin: baseURL, localStorage: [{ name: 'pobr-lang', value: 'en-US' }] }] },
    // A click or fill on a locator that never matches fails here instead of using up the whole test.
    // Waits for a real-WASM import or calculation belong in an explicit `expect(...)` with its own timeout.
    actionTimeout: 30_000,
    navigationTimeout: 30_000,
    screenshot: 'only-on-failure',
    trace: traces ? 'retain-on-failure' : 'off',
  },
  webServer: {
    // preview 用已构建的 dist（含 wasm + public/data）；先 pnpm build + sync-data。
    command: `pnpm preview --port ${port} --strictPort`,
    url: baseURL,
    reuseExistingServer: !custom,
    timeout: 60_000,
  },
});
