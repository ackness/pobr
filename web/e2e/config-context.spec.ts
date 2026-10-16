import { expect, test } from '@playwright/test';
import { deflateSync, inflateSync } from 'node:zlib';

const code = (inputs = '') => deflateSync(`<PathOfBuilding2>
  <Build level="20" className="Witch" mainSocketGroup="1"/>
  <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
  <Skills><Skill enabled="true"><Gem skillId="SparkPlayer" level="5" enabled="true"/></Skill></Skills>
  <Config>${inputs}</Config><Notes>Config visibility fixture</Notes>
</PathOfBuilding2>`).toString('base64url');

test('applicable view hides irrelevant controls, all options exposes them, imported false and zero persist', async ({ page }, testInfo) => {
  await page.goto('/');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill(code());
  await page.locator('.import-submit').click();
  await expect(page.getByRole('heading', { name: 'Items', exact: true })).toBeVisible({ timeout: 60_000 });
  await page.getByRole('button', { name: 'Config', exact: true }).click();
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  const search = page.getByLabel('Search config options…');
  const brands = page.locator('.config-item[data-config-var="ActiveBrands"]');
  await search.fill('ActiveBrands');
  await expect(page.getByRole('button', { name: 'Applicable to this build', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect(brands).toHaveCount(0);
  await page.locator('.config-view').getByRole('button', { name: 'Show all options', exact: true }).click();
  await expect(brands).toBeVisible();
  await expect(brands).toContainText('Default · no saved value');
  await brands.locator('input').fill('0');
  await brands.locator('input').blur();
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await page.getByRole('button', { name: 'Applicable to this build', exact: true }).click();
  await expect(brands).toContainText('Saved value kept visible');
  await expect(brands.locator('input')).toHaveValue('0');

  await page.getByRole('button', { name: 'Build', exact: true }).click();
  await page.getByRole('textbox', { name: 'Build code' }).fill(code('<Input name="ActiveBrands" number="0"/><Input name="BrandsInLastHalf" boolean="false"/>'));
  await page.locator('.import-submit').click();
  await expect(page.getByRole('heading', { name: 'Items', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Config', exact: true }).click();
  await search.fill('Brands');
  await expect(brands).toContainText('Not applicable to this build');
  await expect(brands.locator('input')).toHaveValue('0');
  const half = page.locator('.config-item[data-config-var="BrandsInLastHalf"]');
  await expect(half).toContainText('Saved value kept visible');
  await expect(half.locator('input')).not.toBeChecked();
  await half.scrollIntoViewIfNeeded();
  await page.screenshot({ path: testInfo.outputPath('saved-config.png'), fullPage: true });
  await search.fill('ailmentMode');
  const help = page.locator('.config-item[data-config-var="ailmentMode"]');
  await expect(help.locator('.config-help')).toContainText('Controls how');
  await expect(help.locator('.config-raw-key')).toHaveText('ailmentMode');
  await help.scrollIntoViewIfNeeded();
  await page.screenshot({ path: testInfo.outputPath('config-help.png'), fullPage: true });

  await page.getByRole('button', { name: 'Build', exact: true }).click();
  await page.getByRole('button', { name: 'Generate share code', exact: true }).click();
  const exported = await page.getByRole('textbox', { name: 'Share Code', exact: true }).inputValue();
  const xml = inflateSync(Buffer.from(exported, 'base64url')).toString('utf8');
  expect(xml).toMatch(/<Input name="ActiveBrands" number="0"/);
  expect(xml).toMatch(/<Input name="BrandsInLastHalf" boolean="false"/);
  await page.reload();
  await expect(page.getByRole('button', { name: 'Config', exact: true })).toBeEnabled({ timeout: 90_000 });
  await page.getByRole('button', { name: 'Config', exact: true }).click();
  await search.fill('ActiveBrands');
  await expect(brands.locator('input')).toHaveValue('0');
});

// Observe requests at the JSON/WASM boundary without replacing the real engine.
type ContextRequest = { configContext: boolean; level: number };
type ObservedWindow = Window & { contextRequests: ContextRequest[] };

test('read tracking is requested only while Config is open, including edits and restore', async ({ page }) => {
  await page.addInitScript(() => {
    const requests: ContextRequest[] = [];
    (window as ObservedWindow).contextRequests = requests;
    const stringify = JSON.stringify;
    JSON.stringify = ((value: unknown, ...args: unknown[]) => {
      if (value && typeof value === 'object' && 'character' in value
        && 'socket_groups' in value && 'config_inputs' in value) {
        const request = value as { character: { level: number }; config_context?: boolean };
        requests.push({ configContext: request.config_context === true, level: request.character.level });
      }
      return Reflect.apply(stringify, JSON, [value, ...args]);
    }) as typeof JSON.stringify;
  });
  const requests = () => page.evaluate(() => (window as ObservedWindow).contextRequests);
  const clear = () => page.evaluate(() => { (window as ObservedWindow).contextRequests.length = 0; });
  await page.goto('/');
  const level = page.getByLabel('Level', { exact: true });
  await expect(level).toBeEnabled({ timeout: 90_000 });
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  expect((await requests()).length).toBeGreaterThan(0);
  expect((await requests()).every(request => !request.configContext)).toBe(true);

  await clear();
  await level.fill('21');
  await expect.poll(requests).toContainEqual({ configContext: false, level: 21 });
  expect((await requests()).every(request => !request.configContext)).toBe(true);
  await clear();
  await page.getByRole('button', { name: 'Config', exact: true }).click();
  await expect.poll(requests).toContainEqual({ configContext: true, level: 21 });
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await page.getByLabel('Search config options…').fill('ActiveBrands');
  await expect(page.locator('.config-item[data-config-var="ActiveBrands"]')).toHaveCount(0);

  await clear();
  await page.locator('.config-view').getByRole('button', { name: 'Show all options', exact: true }).click();
  const brands = page.locator('.config-item[data-config-var="ActiveBrands"] input');
  await brands.fill('0');
  await brands.blur();
  await expect.poll(requests).toContainEqual({ configContext: true, level: 21 });
  expect((await requests()).every(request => request.configContext)).toBe(true);
  await page.getByRole('button', { name: 'Build', exact: true }).click();
  await clear();
  await level.fill('23');
  await expect.poll(requests).toContainEqual({ configContext: false, level: 23 });
  expect((await requests()).every(request => !request.configContext)).toBe(true);

  await clear();
  await page.getByRole('button', { name: 'Config', exact: true }).click();
  await expect.poll(requests).toContainEqual({ configContext: true, level: 23 });
  expect(await page.evaluate(() => localStorage.getItem('pobr-build-state'))).not.toContain('"config_context"');
  await page.reload();
  await expect(page.getByLabel('Search config options…')).toBeVisible({ timeout: 90_000 });
  await expect.poll(requests).toContainEqual({ configContext: true, level: 23 });
  expect((await requests()).every(request => request.configContext)).toBe(true);
});
