import { expect, test, type Page } from '@playwright/test';
import { deflateSync } from 'node:zlib';

const code = deflateSync('<PathOfBuilding2><Build level="60" className="Witch" mainSocketGroup="1"/><Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree><Skills activeSkillSet="1"><SkillSet id="1"><Skill enabled="true"><Gem skillId="FireballPlayer" level="12" quality="0" enabled="true"/></Skill></SkillSet></Skills><Items/><Config><Input name="enemyLevel" number="82"/><Input name="buffOnslaught" boolean="false"/></Config></PathOfBuilding2>').toString('base64url');
const nav = (page: Page, name: string) => page.getByRole('navigation', { name: 'Main navigation' }).getByRole('link', { name, exact: true });
/** The build library is an action, not a workflow stage, so it has a button rather than a tab. */
const openBuilds = (page: Page) => page.getByRole('button', { name: 'BDs', exact: true }).click();
const state = (page: Page, key = 'pobr-v2-build-state') => page.evaluate(key => JSON.parse(localStorage.getItem(key)!), key);
async function preferences(page: Page, extra: Record<string, unknown> = {}) {
  await page.addInitScript(extra => {
    if (!localStorage.getItem('pobr-v2-preferences')) localStorage.setItem('pobr-v2-preferences', JSON.stringify({ lang: 'en-US', theme: 'dark', font: 'system', fontSize: 16, density: 'comfortable', ...extra }));
  }, extra);
}
async function importV2(page: Page) {
  await preferences(page);
  await page.goto('/v2/build');
  await expect(page.getByRole('textbox', { name: 'Build code', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();
  await expect(page.locator('#ov-title')).toBeVisible();
  await expect(page.locator('.v2-save-state')).toHaveText('Saved locally');
}

test('V2 starts and preserves saved builds without crypto.randomUUID', async ({ page }) => {
  await page.addInitScript(() => Object.defineProperty(crypto, 'randomUUID', { value: undefined }));
  await importV2(page);
  const imported = await state(page);
  await page.reload();
  await expect(page.locator('#ov-title')).toBeVisible({ timeout: 90_000 });
  await expect(page.locator('.v2-save-state')).toHaveText('Saved locally');
  const restored = await state(page);
  expect(restored.workspace).toEqual(imported.workspace);
  expect(restored.state).toEqual(imported.state);
});

test('V2 copies legacy saves once and config edits preserve the original workspace', async ({ page }) => {
  const dialogs: string[] = []; page.on('dialog', dialog => { dialogs.push(dialog.type()); void dialog.dismiss(); });
  await preferences(page);
  await page.goto('/');
  await expect(page.getByRole('textbox', { name: 'Build code', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click(); await expect(page.locator('.paper-doll')).toBeVisible();
  const original = await page.evaluate(() => localStorage.getItem('pobr-build-state'));
  await page.goto('/v2/config');
  await expect(page.locator('#config-heading')).toBeVisible({ timeout: 90_000 });
  expect((await state(page)).state.params.config_inputs.enemyLevel).toBe(82);
  await page.getByRole('searchbox').fill('enemyLevel');
  const level = page.locator('#config-enemyLevel'); await expect(level).toBeVisible();
  await level.fill('75'); expect((await state(page)).state.params.config_inputs.enemyLevel).toBe(82);
  await level.press('Enter');
  await expect.poll(async () => (await state(page)).state.params.config_inputs.enemyLevel).toBe(75);
  expect(await page.evaluate(() => localStorage.getItem('pobr-build-state'))).toBe(original);
  await page.reload(); await expect(page.locator('#config-heading')).toBeVisible({ timeout: 90_000 });
  expect((await state(page)).state.params.config_inputs.enemyLevel).toBe(75);
  expect(JSON.parse((await page.evaluate(() => localStorage.getItem('pobr-v2-original-backup')))! ).workspace).toBe(original);
  expect(dialogs).toEqual([]);
});

test('V2 navigation, settings and confirmations retain the current editor', async ({ page }) => {
  await importV2(page);
  await expect(page.getByRole('note')).toContainText('configuration');
  await openBuilds(page);
  const notes = page.getByRole('textbox', { name: 'Notes', exact: true });
  await notes.fill('Persistent stage note'); await notes.blur();
  const before = (await state(page)).state;
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByRole('button', { name: 'Light', exact: true }).click();
  await expect(page.locator('html')).not.toHaveClass(/dark/);
  await page.getByRole('slider', { name: /Text size/ }).focus(); await page.keyboard.press('End');
  await expect(page.locator('html')).toHaveCSS('font-size', '32px');
  await page.getByRole('slider', { name: /Text size/ }).focus(); await page.keyboard.press('Home');
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(notes).toHaveValue('Persistent stage note');
  expect((await state(page)).state).toEqual(before);
  await nav(page, 'Calcs').click(); await expect(page).toHaveURL(/\/v2\/calcs$/);
  await page.goBack(); await expect(page.locator('#build-heading')).toBeVisible();
  await expect(notes).toHaveValue('Persistent stage note');
  await page.getByRole('button', { name: 'Copy current stage', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'Name', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await page.getByRole('combobox', { name: 'Class', exact: true }).click();
  await page.getByRole('option', { name: 'Warrior', exact: true }).click();
  await expect(page.getByRole('alertdialog')).toBeVisible();
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  expect((await state(page)).state.character.class_name).toBe('Witch');
  expect((await state(page)).workspace.builds.at(-1).stages).toHaveLength(1);
});

for (const viewport of [{ width: 390, size: 32 }, { width: 1440, size: 16 }, { width: 3440, size: 16 }]) {
  test(`V2 functional pages fit ${viewport.width}px with ${viewport.size}px text`, async ({ page }) => {
    await page.setViewportSize({ width: viewport.width, height: 900 });
    await preferences(page, { fontSize: viewport.size }); await importV2(page);
    const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
    for (const route of ['overview', 'build', 'skills', 'items', 'calcs', 'config', 'compare', 'trade', 'tree']) {
      await page.goto(route === 'overview' ? '/v2' : `/v2/${route}`);
      await expect(page.locator('.v2-main')).toBeVisible({ timeout: 90_000 });
      await expect(page.locator('.v2-panel-loading')).toHaveCount(0);
      const widths = await page.locator('main').evaluate(element => ({ client: element.clientWidth, scroll: element.scrollWidth }));
      expect(widths.scroll, `${route} has page-wide overflow`).toBeLessThanOrEqual(widths.client + 1);
      expect(await page.locator('main [title]').count(), `${route} uses browser tooltips`).toBe(0);
    }
    if (viewport.width === 390) {
      await page.getByRole('button', { name: 'Open menu', exact: true }).click();
      await page.getByRole('button', { name: 'Appearance & language', exact: true }).click();
      await page.setViewportSize({ width: 390, height: 400 });
      await page.getByRole('button', { name: 'Light', exact: true }).click();
      await expect(page.locator('body')).toHaveCSS('background-color', 'rgb(247, 246, 243)');
      const sheet = page.locator('.v2-settings');
      expect(await sheet.evaluate(element => element.scrollWidth <= element.clientWidth + 1)).toBe(true);
      await page.getByRole('button', { name: 'Close', exact: true }).click();
      await expect(sheet).toHaveCount(0);
    }
    expect(errors).toEqual([]);
  });
}

test('browser Back closes portals owned by retained skill panels', async ({ page }) => {
  await importV2(page);
  await nav(page, 'Skills').click();
  const sets = page.getByRole('combobox', { name: 'Skill set', exact: true });
  await sets.click(); await expect(page.getByRole('listbox')).toBeVisible();
  await page.goBack(); await expect(page.locator('#ov-title')).toBeVisible();
  await expect(page.getByRole('listbox')).toHaveCount(0);
  await nav(page, 'Skills').click();
  await expect(sets).toHaveAttribute('aria-expanded', 'false');
});

test('V2 real WASM details stay lazy and configuration tracking follows the active route', async ({ page }) => {
  await page.addInitScript(() => {
    const observed = window as typeof window & { v2CalcRequests: Array<{ breakdowns?: string[]; config_context?: boolean }> };
    observed.v2CalcRequests = [];
    const record = (value: unknown) => {
      if (value && typeof value === 'object' && 'character' in value && 'socket_groups' in value && 'config_inputs' in value) {
        observed.v2CalcRequests.push(value);
      }
    };
    // On the page's own thread a request is stringified for the engine; with the engine in a Worker it is posted as an RPC call.
    const stringify = JSON.stringify;
    JSON.stringify = ((value: unknown, ...args: unknown[]) => {
      record(value);
      return Reflect.apply(stringify, JSON, [value, ...args]);
    }) as typeof JSON.stringify;
    const post = Worker.prototype.postMessage;
    Worker.prototype.postMessage = function (this: Worker, message: unknown, ...rest: unknown[]) {
      const args = (message as { args?: unknown } | null)?.args;
      if (Array.isArray(args)) args.forEach(record);
      return Reflect.apply(post, this, [message, ...rest]);
    } as typeof Worker.prototype.postMessage;
  });
  const requests = () => page.evaluate(() => (window as typeof window & { v2CalcRequests: Array<{ breakdowns?: string[]; config_context?: boolean; breakdown_actor?: unknown }> }).v2CalcRequests);
  await importV2(page);
  await nav(page, 'Calcs').click();
  await expect(page.locator('#breakdown-Life')).toBeVisible();
  expect((await requests()).filter(request => request.breakdowns?.length)).toHaveLength(0);
  await page.locator('#breakdown-Life').click();
  await expect(page.locator('#calcs-detail .formula-final')).toBeVisible();
  expect((await requests()).filter(request => request.breakdowns?.length)).toEqual([
    expect.objectContaining({ breakdowns: ['Life'], breakdown_actor: { kind: 'player' } }),
  ]);
  await nav(page, 'Config').click();
  await expect.poll(async () => (await requests()).at(-1)?.config_context).toBe(true);
  await openBuilds(page);
  await page.getByLabel('Level', { exact: true }).fill('61');
  await expect(page.locator('.v2-save-state')).toHaveText('Saved locally');
  expect((await requests()).at(-1)?.config_context).not.toBe(true);
  expect((await state(page)).state.character.level).toBe(61);
});
