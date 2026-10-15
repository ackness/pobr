import { deflateSync } from 'node:zlib';
import { expect, test } from '@playwright/test';
import { comparisonWorkspace } from './fixtures/compare-workspace';
import { workspaceEnvelope } from '../src/lib/buildWorkspace';

test('compare stages without mutation, share A config, and apply one item to inactive A only', async ({ page }, testInfo) => {
  await page.addInitScript(raw => { if (!localStorage.getItem('pobr-build-state')) localStorage.setItem('pobr-build-state', raw); }, workspaceEnvelope(comparisonWorkspace()));
  await page.goto('/');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  const snapshot = () => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!).workspace);
  const original = await snapshot();
  await page.getByRole('button', { name: 'Compare', exact: true }).click();
  await page.getByRole('combobox', { name: 'Build / stage A', exact: true }).selectOption('stage-a');
  await page.getByRole('combobox', { name: 'Build / stage B', exact: true }).selectOption('stage-b');
  const life = page.locator('.compare-summary [data-stat="Life"]');
  await expect(life.locator('td').nth(2)).toHaveText(/^\+[0-9]+/);
  const ownDelta = Number((await life.locator('td').nth(2).innerText()).replaceAll(',', ''));
  await expect(page.getByRole('checkbox', { name: 'Only changed stats', exact: true })).toBeChecked();
  expect(await snapshot()).toEqual(original);
  await page.getByRole('combobox', { name: 'Calculation config', exact: true }).selectOption('a');
  await expect.poll(async () => Number((await life.locator('td').nth(2).innerText()).replaceAll(',', ''))).toBeLessThan(ownDelta);
  expect(Number((await life.locator('td').nth(2).innerText()).replaceAll(',', ''))).toBeGreaterThan(0);
  expect(await snapshot()).toEqual(original);
  const item = page.locator('[data-item="items:ring1"]');
  await expect(item.locator('.compare-line-before')).toContainText('+100 to maximum Life');
  await expect(item.locator('.compare-line-after')).toContainText('+200 to maximum Life');
  await item.getByRole('button', { name: 'Apply B → A', exact: true }).click();
  await expect(life).toHaveCount(0);
  await expect(item).toHaveCount(0);
  await page.getByRole('checkbox', { name: 'Only changed stats', exact: true }).uncheck();
  await expect(life.locator('td').nth(2)).toHaveText('0');
  expect(await life.locator('td').nth(0).innerText()).toBe(await life.locator('td').nth(1).innerText());
  const copied = await snapshot();
  expect(copied.activeBuild).toBe(original.activeBuild);
  expect(copied.builds[0].activeStage).toBe('stage-b');
  expect(copied.builds[0].stages[1]).toEqual(original.builds[0].stages[1]);
  expect(copied.builds[0].stages[0]).toEqual({ ...original.builds[0].stages[0], saved: {
    ...original.builds[0].stages[0].saved, state: { ...original.builds[0].stages[0].saved.state,
      items: original.builds[0].stages[1].saved.state.items, sourceBackfillComplete: true },
  } });
  await page.getByRole('heading', { name: 'Compare builds and stages', exact: true }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: testInfo.outputPath('comparison.png') });
  await page.setViewportSize({ width: 640, height: 900 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.reload();
  await expect(page.getByRole('heading', { name: 'Compare builds and stages', exact: true })).toBeVisible({ timeout: 90_000 });
  expect((await snapshot()).builds[0].stages[0].saved.state.items).toEqual(original.builds[0].stages[1].saved.state.items);
});


test('copying an absent config value stays cleared when an imported stage is reopened', async ({ page }) => {
  const workspace = comparisonWorkspace();
  const a = workspace.builds[0].stages[0];
  a.saved.state.pobCode = deflateSync(`<PathOfBuilding2><Build level="20" className="Ranger" mainSocketGroup="1"/>
    <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
    <Skills><Skill enabled="true"><Gem skillId="SparkPlayer" level="5" enabled="true"/></Skill></Skills>
    <Config><Input name="conditionLowLife" boolean="true"/></Config></PathOfBuilding2>`).toString('base64url');
  a.saved.state.params.config_inputs.conditionLowLife = true;
  await page.addInitScript(raw => localStorage.setItem('pobr-build-state', raw), workspaceEnvelope(workspace));
  await page.goto('/');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await page.getByRole('button', { name: 'Compare', exact: true }).click();
  await page.getByRole('combobox', { name: 'Build / stage A', exact: true }).selectOption('stage-a');
  await page.getByRole('combobox', { name: 'Build / stage B', exact: true }).selectOption('stage-b');
  const row = page.locator('[data-config-key="conditionLowLife"]');
  await expect(row).toContainText('Not set');
  await row.getByRole('button', { name: 'Apply B → A', exact: true }).click();
  await expect(row).toHaveCount(0);
  await page.getByRole('combobox', { name: 'Stage', exact: true }).selectOption('stage-a');
  await page.getByRole('button', { name: 'Build', exact: true }).click();
  await expect(page.locator('.import-summary')).toBeVisible();
  const restored = await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!));
  expect(restored.state.params.config_inputs).not.toHaveProperty('conditionLowLife');
  expect(restored.state.pobCode).toBe(a.saved.state.pobCode);
  expect(restored.workspace.builds[0].stages[1]).toEqual(workspace.builds[0].stages[1]);
});
