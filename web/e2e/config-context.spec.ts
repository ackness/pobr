import { expect, test } from '@playwright/test';
import { go, importCode, shareCode } from './configPage';
import { deflateSync, inflateSync } from 'node:zlib';
import { readFileSync } from 'node:fs';
import { QUARANTINE } from './tags';

const code = (inputs = '') => deflateSync(`<PathOfBuilding2>
  <Build level="20" className="Witch" mainSocketGroup="1"/>
  <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
  <Skills><Skill enabled="true"><Gem skillId="SparkPlayer" level="5" enabled="true"/></Skill></Skills>
  <Config>${inputs}</Config><Notes>Config visibility fixture</Notes>
</PathOfBuilding2>`).toString('base64url');

test('editing a deep combat option keeps the scroll position throughout recalculation', async ({ page }) => {
  const source = readFileSync(new URL('../../examples/wasm-benchmark/builds/ninja-2026-10-11-ranger-deadeye-ice-shot-b6ce7b3a4329/code.txt', import.meta.url), 'utf8').trim();
  await page.goto('/build');
  await importCode(page, source);
  await go(page, 'Configuration');
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  const option = page.locator('#config-conditionCritRecently');
  await option.scrollIntoViewIfNeeded();
  const top = await page.locator('#shell-main').evaluate(el => el.scrollTop);
  expect(top).toBeGreaterThan(100);
  // Capture every rendered frame: checking only the eventual layout misses the
  // short-lived collapse that clamps scrollTop before the result comes back.
  await page.evaluate(() => {
    const root = document.querySelector('#shell-main')!;
    const state = { active: true, positions: [root.scrollTop] };
    (window as unknown as { editScroll: typeof state }).editScroll = state;
    const sample = () => { state.positions.push(root.scrollTop); if (state.active) requestAnimationFrame(sample); };
    requestAnimationFrame(sample);
  });
  await option.click();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  const positions = await page.evaluate(() => {
    const state = (window as unknown as { editScroll: { active: boolean; positions: number[] } }).editScroll;
    state.active = false;
    return state.positions;
  });
  expect(Math.min(...positions)).toBeGreaterThanOrEqual(top - 2);
  expect(await page.locator('#shell-main').evaluate(el => el.scrollTop)).toBeCloseTo(top, 0);
  await go(page, 'Calculations');
  await page.getByRole('button', { name: 'All metrics', exact: true }).click();
  const metrics = page.locator('.calcs-metric-list');
  await metrics.evaluate(el => { el.scrollTop = 800; });
  expect(await metrics.evaluate(el => el.scrollTop)).toBe(800);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  expect(await metrics.evaluate(el => el.scrollTop)).toBe(800);
});

test('applicable view hides irrelevant controls, all options exposes them, imported false and zero persist', async ({ page }, testInfo) => {
  await page.goto('/build');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await importCode(page, code());
  await expect(page.getByRole('heading', { name: 'Items', exact: true })).toBeVisible({ timeout: 60_000 });
  await go(page, 'Configuration');
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  const search = page.getByLabel('Search config options…');
  const stacks = page.locator('.config-item[data-config-var="demonFormStacks"]');
  await search.fill('demonFormStacks');
  await expect(page.getByRole('radio', { name: 'Relevant to this build', exact: true })).toHaveAttribute('aria-checked', 'true');
  await expect(stacks).toHaveCount(0);
  await page.locator('.config-view').getByRole('radio', { name: 'All', exact: true }).click();
  await expect(stacks).toBeVisible();
  await expect(stacks).toContainText('Default · no saved value');
  await stacks.locator('input').fill('0');
  await stacks.locator('input').blur();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await page.getByRole('radio', { name: 'Relevant to this build', exact: true }).click();
  if (!(await stacks.locator('.config-option-help').evaluate(element => (element as HTMLDetailsElement).open))) await stacks.locator('summary').click();
  await expect(stacks).toContainText('Saved value kept visible');
  await expect(stacks.locator('input')).toHaveValue('0');

  await importCode(page, code('<Input name="demonFormStacks" number="0"/><Input name="BrandsInLastHalf" boolean="false"/>'));
  await expect(page.getByRole('heading', { name: 'Items', exact: true })).toBeVisible();
  await go(page, 'Configuration');
  await search.fill('demonFormStacks');
  if (!(await stacks.locator('.config-option-help').evaluate(element => (element as HTMLDetailsElement).open))) await stacks.locator('summary').click();
  await expect(stacks).toContainText('Not relevant to this build');
  await expect(stacks.locator('input')).toHaveValue('0');
  await search.fill('BrandsInLastHalf');
  const half = page.locator('.config-item[data-config-var="BrandsInLastHalf"]');
  await half.locator('summary').click();
  await expect(half).toContainText('Saved value kept visible');
  await expect(half.locator('input')).not.toBeChecked();
  await half.scrollIntoViewIfNeeded();
  await page.screenshot({ path: testInfo.outputPath('saved-config.png'), fullPage: true });
  await search.fill('ailmentMode');
  const help = page.locator('.config-item[data-config-var="ailmentMode"]');
  // The help and the internal key sit behind the row's "Help" disclosure.
  await help.locator('summary').click();
  await expect(help.locator('.config-help')).toContainText('Controls how');
  await expect(help.locator('.config-raw-key')).toHaveText('ailmentMode');
  await help.scrollIntoViewIfNeeded();
  await page.screenshot({ path: testInfo.outputPath('config-help.png'), fullPage: true });

  const exported = await shareCode(page);
  const xml = inflateSync(Buffer.from(exported, 'base64url')).toString('utf8');
  expect(xml).toMatch(/<Input name="demonFormStacks" number="0"/);
  expect(xml).toMatch(/<Input name="BrandsInLastHalf" boolean="false"/);
  await page.reload();
  await go(page, 'Configuration');
  await search.fill('demonFormStacks');
  await expect(stacks.locator('input')).toHaveValue('0');
});

// Observe requests at the JSON/WASM boundary without replacing the real engine.
type ContextRequest = { configContext: boolean; level: number };
type ObservedWindow = Window & { contextRequests: ContextRequest[] };

test('read tracking is requested only while Config is open, including edits and restore', async ({ page }) => {
  await page.addInitScript(() => {
    const requests: ContextRequest[] = [];
    (window as unknown as ObservedWindow).contextRequests = requests;
    const observe = (value: unknown) => {
      if (value && typeof value === 'object' && 'character' in value
        && 'socket_groups' in value && 'config_inputs' in value) {
        const request = value as { character: { level: number }; config_context?: boolean };
        requests.push({ configContext: request.config_context === true, level: request.character.level });
      }
    };
    // The engine runs in a worker: its calculations are requested by a message from the page.
    const post = Worker.prototype.postMessage;
    Worker.prototype.postMessage = function (this: Worker, message: unknown, ...rest: unknown[]) {
      const call = message as { method?: string; args?: unknown[] } | null;
      if (call?.method === 'calculateBuild') observe(call.args?.[0]);
      return Reflect.apply(post, this, [message, ...rest]);
    } as typeof Worker.prototype.postMessage;
    // On a page where the worker cannot start, the engine runs on the page's own thread instead.
    const stringify = JSON.stringify;
    JSON.stringify = ((value: unknown, ...args: unknown[]) => {
      observe(value);
      return Reflect.apply(stringify, JSON, [value, ...args]);
    }) as typeof JSON.stringify;
  });
  const requests = () => page.evaluate(() => (window as unknown as ObservedWindow).contextRequests);
  const clear = () => page.evaluate(() => { (window as unknown as ObservedWindow).contextRequests.length = 0; });
  await page.goto('/build');
  const level = page.getByLabel('Level', { exact: true });
  await expect(level).toBeEnabled({ timeout: 90_000 });
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  expect((await requests()).length).toBeGreaterThan(0);
  expect((await requests()).every(request => !request.configContext)).toBe(true);

  await clear();
  await level.fill('21');
  await expect.poll(requests).toContainEqual({ configContext: false, level: 21 });
  expect((await requests()).every(request => !request.configContext)).toBe(true);
  await clear();
  await go(page, 'Configuration');
  await expect.poll(requests).toContainEqual({ configContext: true, level: 21 });
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await page.getByLabel('Search config options…').fill('demonFormStacks');
  await expect(page.locator('.config-item[data-config-var="demonFormStacks"]')).toHaveCount(0);

  await clear();
  await page.locator('.config-view').getByRole('radio', { name: 'All', exact: true }).click();
  const stacks = page.locator('.config-item[data-config-var="demonFormStacks"] input');
  await stacks.fill('0');
  await stacks.blur();
  await expect.poll(requests).toContainEqual({ configContext: true, level: 21 });
  expect((await requests()).every(request => request.configContext)).toBe(true);
  await page.locator('.shell-builds-button').click();
  await clear();
  await level.fill('23');
  await expect.poll(requests).toContainEqual({ configContext: false, level: 23 });
  expect((await requests()).every(request => !request.configContext)).toBe(true);

  await clear();
  await go(page, 'Configuration');
  await expect.poll(requests).toContainEqual({ configContext: true, level: 23 });
  expect(await page.evaluate(() => localStorage.getItem('pobr-v2-build-state'))).not.toContain('"config_context"');
  await page.reload();
  await expect(page.getByLabel('Search config options…')).toBeVisible({ timeout: 90_000 });
  await expect.poll(requests).toContainEqual({ configContext: true, level: 23 });
  expect((await requests()).every(request => request.configContext)).toBe(true);
});

// Quarantined: the config page now shows one group at a time (eb997114), so the dependent option is hidden until
// its group is chosen and the "folding the leading group" step no longer applies. Rewrite around the group menu.
test('an option that depends on the one just set appears in its own group, not in "Relevant to this BD"', QUARANTINE, async ({ page }) => {
  await page.goto('/build');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await importCode(page, code());
  await expect(page.getByRole('heading', { name: 'Items', exact: true })).toBeVisible({ timeout: 60_000 });
  await go(page, 'Configuration');
  await page.locator('.config-view').getByRole('radio', { name: 'All', exact: true }).click();
  const dependent = page.locator('#config-section-When-In-Combat .config-item[data-config-var="conditionChilledEffect"]');
  const promoted = page.locator('#config-section-Relevant-to-this-build .config-item[data-config-var="conditionChilledEffect"]');
  await expect(dependent).toBeVisible();
  // `conditionChilledEffect` is only meant once `conditionChilled` is set. Setting it must not move the dependent
  // to the top of the page: it stays in its own group, next to the control just changed.
  await page.locator('#config-conditionChilled').setChecked(true);
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await expect(dependent).toBeVisible();
  await expect(promoted).toHaveCount(0);
  // So folding the leading group does not take it out of reach either.
  const relevant = page.locator('#config-section-Relevant-to-this-build .config-section-header');
  await relevant.click();
  await expect(relevant).toHaveAttribute('aria-expanded', 'false');
  await expect(dependent).toBeVisible();
});
