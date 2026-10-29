import { expect, test } from '@playwright/test';
import { openItemsAfterImport } from './shell';
import { choose, openBuild, openExport } from './buildLibrary';
import { readFileSync } from 'node:fs';
import { deflateSync } from 'node:zlib';

const fixture = readFileSync(new URL('../../apps/pobr-wasm/tests/fixtures/pob2-planner.build', import.meta.url), 'utf8');

test('planner file import, reload and browser export preserve hints and keep gear empty', async ({ page }) => {
  await page.goto('/build');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await page.locator('input[type=file]').setInputFiles({ name: 'Writer fixture.build', mimeType: 'application/json', buffer: Buffer.from(fixture) });
  await expect(page.locator('#ov-title')).toBeVisible();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  const before = await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!));
  expect(before.state.items).toEqual([]);
  expect(before.state.socketGroups[0].gems[0]).toMatchObject({ skill_id: 'IceShotPlayer', level: 17, quality: 12 });
  expect(JSON.parse(before.state.plannerSource)).toEqual(JSON.parse(fixture));
  await page.reload();
  await expect(page.locator('#ov-title')).toBeVisible({ timeout: 90_000 });
  await openBuild(page);
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await openExport(page);
  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Download current .build', exact: true }).click();
  const file = await download;
  expect(file.suggestedFilename()).toMatch(/\.build$/);
  const content = readFileSync((await file.path())!, 'utf8');
  const exported = JSON.parse(content);
  expect(exported.inventory_slots).toEqual(JSON.parse(fixture).inventory_slots);
  expect(exported.author).toBe('Fixture author');
  await page.screenshot({ path: 'test-results/planner-file-export.png', fullPage: true });
  await page.locator('input[type=file]').setInputFiles({ name: file.suggestedFilename(), mimeType: 'application/json', buffer: Buffer.from(content) });
  await expect(page.locator('#ov-title')).toBeVisible();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  const after = await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!));
  expect(after.state.items).toEqual([]);
  expect(after.state.allocatedNodes).toEqual(before.state.allocatedNodes);
  expect(after.state.weaponSwap).toEqual(before.state.weaponSwap);
  expect(after.state.socketGroups).toEqual(before.state.socketGroups);
});

test('all source loadouts provide separate downloadable files without switching the stage', async ({ page }) => {
  await page.goto('/build');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  const code = deflateSync('<PathOfBuilding2><Build level="50" className="Ranger" ascendClassName="Deadeye"/><Tree activeSpec="1"><Spec title="Starter" nodes="3936" treeVersion="0_5"/><Spec title="Endgame" nodes="46325" treeVersion="0_5"/></Tree><Skills/><Items/></PathOfBuilding2>').toString('base64url');
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();
  await openItemsAfterImport(page);
  await expect(page.locator('.paper-doll')).toBeVisible();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await openBuild(page);
  await openExport(page);
  await page.getByRole('button', { name: 'Prepare all loadouts (.build)', exact: true }).click();
  const files = page.getByRole('button', { name: /\.build$/ });
  await expect(files).toHaveCount(3); // current-download action plus two prepared files
  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: /Endgame.*\.build$/ }).click();
  const file = await download;
  const exported = JSON.parse(readFileSync((await file.path())!, 'utf8'));
  expect(exported.passives[0].id).toBe('melee18');
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state.allocatedNodes)).toEqual([3936]);
});

test('a planner without ascendancy can choose its class without discarding the imported plan', async ({ page }) => {
  await page.goto('/build');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  const source = JSON.parse(fixture);
  delete source.ascendancy;
  await page.locator('input[type=file]').setInputFiles({ name: 'Choose class.build', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(source)) });
  await expect(page.locator('#ov-title')).toBeVisible();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await openBuild(page);
  await choose(page, page.getByRole('combobox', { name: 'Class', exact: true }), 'Ranger');
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  const state = await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state);
  expect(state.character.class_name).toBe('Ranger');
  expect(state.allocatedNodes).toEqual([3936]);
  expect(state.socketGroups[0].gems[0].skill_id).toBe('IceShotPlayer');
});
