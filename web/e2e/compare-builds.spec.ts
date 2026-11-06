import { deflateSync } from 'node:zlib';
import { expect, test } from '@playwright/test';
import { comparisonWorkspace } from './fixtures/compare-workspace';
import { busy, pageLink } from './shell';
import { choose, compareStageA, compareStageB, compareTab } from './treePage';
import { workspaceEnvelope } from '../src/lib/buildWorkspace';

test('compare stages without mutation, share A config, and apply one item to inactive A only', async ({ page }, testInfo) => {
  await page.addInitScript(raw => { if (!localStorage.getItem('pobr-v2-build-state')) localStorage.setItem('pobr-v2-build-state', raw); }, workspaceEnvelope(comparisonWorkspace()));
  await page.goto('/build');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await expect(busy(page)).toHaveCount(0);
  const snapshot = () => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).workspace);
  const original = await snapshot();
  await pageLink(page, 'Compare').click();
  await choose(page, compareStageA(page), 'Spark / Starter');
  await choose(page, compareStageB(page), 'Spark / Target');
  const life = page.locator('.compare-summary [data-stat="Life"]');
  await expect(life.locator('td').nth(2)).toHaveText(/^\+[0-9]+/);
  const ownDelta = Number((await life.locator('td').nth(2).innerText()).replaceAll(',', ''));
  await expect(page.getByRole('checkbox', { name: 'Only changed stats', exact: true })).toBeChecked();
  expect(await snapshot()).toEqual(original);
  await choose(page, page.getByRole('combobox', { name: 'Calculation config', exact: true }), 'Use A’s config for both');
  await expect.poll(async () => Number((await life.locator('td').nth(2).innerText()).replaceAll(',', ''))).toBeLessThan(ownDelta);
  expect(Number((await life.locator('td').nth(2).innerText()).replaceAll(',', ''))).toBeGreaterThan(0);
  expect(await snapshot()).toEqual(original);
  // Only the open section is mounted: the item difference lives under Items, the stat rows under Stats.
  await compareTab(page, 'Items').click();
  const item = page.locator('[data-item="items:ring1"]');
  await expect(item.locator('.compare-line-before')).toContainText('+100 to maximum Life');
  await expect(item.locator('.compare-line-after')).toContainText('+200 to maximum Life');
  await item.getByRole('button', { name: 'Apply B → A', exact: true }).click();
  await expect(item).toHaveCount(0);
  await compareTab(page, 'Stats').click();
  await expect(life).toHaveCount(0);
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
  await page.getByRole('heading', { name: 'Compare', exact: true }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: testInfo.outputPath('comparison.png') });
  await page.setViewportSize({ width: 640, height: 900 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.reload();
  await expect(page.getByRole('heading', { name: 'Compare', exact: true })).toBeVisible({ timeout: 90_000 });
  expect((await snapshot()).builds[0].stages[0].saved.state.items).toEqual(original.builds[0].stages[1].saved.state.items);
});


for (const activeStage of ['stage-a', 'stage-b']) {
  test(`unset B cannot remove config from ${activeStage === 'stage-a' ? 'active' : 'inactive'} A`, async ({ page }) => {
    const workspace = comparisonWorkspace();
    workspace.builds[0].activeStage = activeStage;
    const a = workspace.builds[0].stages[0];
    const shockKey = 'conditionEnemyShocked';
    const questKey = 'questAct 1ClearfellBeira';
    a.saved.state.pobCode = deflateSync(`<PathOfBuilding2><Build level="20" className="Ranger" mainSocketGroup="1"/>
      <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
      <Skills><Skill enabled="true"><Gem skillId="SparkPlayer" level="5" enabled="true"/></Skill></Skills>
      <Config><Input name="${shockKey}" boolean="true"/><Input name="${questKey}" boolean="false"/></Config>
      </PathOfBuilding2>`).toString('base64url');
    Object.assign(a.saved.state.params.config_inputs, { [shockKey]: true, [questKey]: false });
    await page.addInitScript(raw => {
      if (!localStorage.getItem('pobr-v2-build-state')) localStorage.setItem('pobr-v2-build-state', raw);
    }, workspaceEnvelope(workspace));
    await page.goto('/build');
    await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
    await expect(busy(page)).toHaveCount(0);
    const snapshot = () => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).workspace);
    const original = await snapshot();
    await pageLink(page, 'Compare').click();
    await choose(page, compareStageA(page), 'Spark / Starter');
    await choose(page, compareStageB(page), 'Spark / Target');
    await compareTab(page, 'Configuration').click();
    for (const key of [shockKey, questKey]) {
      const difference = page.locator(`[data-config-key="${key}"]`);
      await expect(difference).toContainText('Not set');
      await expect(difference.getByRole('button', { name: 'B is not set; nothing to apply', exact: true })).toBeDisabled();
    }
    expect(await snapshot()).toEqual(original);
    await page.reload();
    await expect(page.getByRole('heading', { name: 'Compare', exact: true })).toBeVisible({ timeout: 90_000 });
    await expect(busy(page)).toHaveCount(0);
    const restored = await snapshot();
    expect(restored.builds[0].stages[0].saved.state.params.config_inputs).toEqual(original.builds[0].stages[0].saved.state.params.config_inputs);
    expect(restored.builds[0].stages[1]).toEqual(original.builds[0].stages[1]);
  });
}

test('selecting B from the empty comparison keeps its picker mounted and focused', async ({ page }) => {
  const workspace = comparisonWorkspace();
  const original = workspace.builds[0];
  const [starter, target] = original.stages;
  workspace.builds = [
    { ...original, activeStage: starter.id, stages: [starter] },
    { ...original, id: 'separate-build', name: 'Target build', activeStage: target.id, stages: [target] },
  ];
  workspace.activeBuild = 'separate-build';
  await page.addInitScript(raw => localStorage.setItem('pobr-v2-build-state', raw), workspaceEnvelope(workspace));
  await page.goto('/compare');
  await expect(page.locator('.cmp-empty')).toBeVisible({ timeout: 90_000 });
  const picker = compareStageB(page);
  const originalPicker = await picker.elementHandle();
  await picker.focus();
  await picker.press('Enter');
  // Radix focuses the selected option after positioning, then defers arrow-key focus changes.
  await expect(page.getByRole('option', { name: 'Choose a build / stage for B', exact: true })).toBeFocused();
  await page.keyboard.press('ArrowDown');
  await expect(page.getByRole('option', { name: 'Spark / Starter', exact: true })).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('listbox')).toHaveCount(0);
  await expect(picker).toContainText('Spark / Starter');
  await expect(page.locator('.cmp-empty')).toHaveCount(0);
  await expect(picker).toBeFocused();
  expect(await originalPicker!.evaluate(node => node.isConnected && node === document.activeElement)).toBe(true);
});
