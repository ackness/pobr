import { expect, test } from '@playwright/test';
import { go, importCode, shareCode } from './configPage';
import { deflateSync, inflateSync } from 'node:zlib';

const code = (inputs = '') => deflateSync(`<PathOfBuilding2>
  <Build level="20" className="Witch" mainSocketGroup="1"/>
  <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
  <Skills><Skill enabled="true"><Gem skillId="SparkPlayer" level="5" enabled="true"/></Skill></Skills>
  <Config>${inputs}</Config><Notes>Config visibility fixture</Notes>
</PathOfBuilding2>`).toString('base64url');

test('applicable view hides irrelevant controls, all options exposes them, imported false and zero persist', async ({ page }, testInfo) => {
  await page.goto('/build');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await importCode(page, code());
  await expect(page.getByRole('heading', { name: 'Items', exact: true })).toBeVisible({ timeout: 60_000 });
  await go(page, 'Config');
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  const search = page.getByLabel('Search config options…');
  const stacks = page.locator('.config-item[data-config-var="demonFormStacks"]');
  await search.fill('demonFormStacks');
  await expect(page.getByRole('button', { name: 'Applicable to this build', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect(stacks).toHaveCount(0);
  await page.locator('.config-view').getByRole('button', { name: 'Show all options', exact: true }).click();
  await expect(stacks).toBeVisible();
  await expect(stacks).toContainText('Default · no saved value');
  await stacks.locator('input').fill('0');
  await stacks.locator('input').blur();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await page.getByRole('button', { name: 'Applicable to this build', exact: true }).click();
  await expect(stacks).toContainText('Saved value kept visible');
  await expect(stacks.locator('input')).toHaveValue('0');

  await importCode(page, code('<Input name="demonFormStacks" number="0"/><Input name="BrandsInLastHalf" boolean="false"/>'));
  await expect(page.getByRole('heading', { name: 'Items', exact: true })).toBeVisible();
  await go(page, 'Config');
  await search.fill('demonFormStacks');
  await expect(stacks).toContainText('Not applicable to this build');
  await expect(stacks.locator('input')).toHaveValue('0');
  await search.fill('BrandsInLastHalf');
  const half = page.locator('.config-item[data-config-var="BrandsInLastHalf"]');
  await expect(half).toContainText('Saved value kept visible');
  await expect(half.locator('input')).not.toBeChecked();
  await half.scrollIntoViewIfNeeded();
  await page.screenshot({ path: testInfo.outputPath('saved-config.png'), fullPage: true });
  await search.fill('ailmentMode');
  const help = page.locator('.config-item[data-config-var="ailmentMode"]');
  // The help and the internal key sit behind the row's "Help & source" disclosure.
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
  await go(page, 'Config');
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
  await go(page, 'Config');
  await expect.poll(requests).toContainEqual({ configContext: true, level: 21 });
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await page.getByLabel('Search config options…').fill('demonFormStacks');
  await expect(page.locator('.config-item[data-config-var="demonFormStacks"]')).toHaveCount(0);

  await clear();
  await page.locator('.config-view').getByRole('button', { name: 'Show all options', exact: true }).click();
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
  await go(page, 'Config');
  await expect.poll(requests).toContainEqual({ configContext: true, level: 23 });
  expect(await page.evaluate(() => localStorage.getItem('pobr-v2-build-state'))).not.toContain('"config_context"');
  await page.reload();
  await expect(page.getByLabel('Search config options…')).toBeVisible({ timeout: 90_000 });
  await expect.poll(requests).toContainEqual({ configContext: true, level: 23 });
  expect((await requests()).every(request => request.configContext)).toBe(true);
});
