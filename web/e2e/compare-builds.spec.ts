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
  await page.getByRole('heading', { name: 'Compare builds and stages', exact: true }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: testInfo.outputPath('comparison.png') });
  await page.setViewportSize({ width: 640, height: 900 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.reload();
  await expect(page.getByRole('heading', { name: 'Compare builds and stages', exact: true })).toBeVisible({ timeout: 90_000 });
  expect((await snapshot()).builds[0].stages[0].saved.state.items).toEqual(original.builds[0].stages[1].saved.state.items);
});


for (const activeStage of ['stage-a', 'stage-b']) {
  test(`copying unset config into ${activeStage === 'stage-a' ? 'active' : 'inactive'} A keeps controls, defaults and reset consistent`, async ({ page }) => {
    const workspace = comparisonWorkspace();
    workspace.builds[0].activeStage = activeStage;
    const a = workspace.builds[0].stages[0];
    const shockKey = 'conditionEnemyShocked';
    const questKey = 'questAct 1ClearfellBeira';
    a.saved.state.pobCode = deflateSync(`<PathOfBuilding2><Build level="20" className="Ranger" mainSocketGroup="1"/>
      <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
      <Skills><Skill enabled="true"><Gem skillId="SparkPlayer" level="5" enabled="true"/></Skill></Skills>
      <Config activeConfigSet="7"><ConfigSet id="7" title="Imported">
        <Input name="${shockKey}" boolean="true"/><Input name="${questKey}" boolean="false"/>
        <Input name="conditionFullLife" boolean="false"/><Input name="ActiveBrands" number="0"/>
        <Input name="futureInput" string="20"/>
      </ConfigSet><ConfigSet id="3" title="Other"><Input name="${shockKey}" boolean="false"/></ConfigSet></Config>
      </PathOfBuilding2>`).toString('base64url');
    Object.assign(a.saved.state.params.config_inputs, { [shockKey]: true, [questKey]: false, futureInput: '20' });
    // A reload must read the result of Compare, not reseed the initial fixture.
    await page.addInitScript(raw => {
      if (!localStorage.getItem('pobr-v2-build-state')) localStorage.setItem('pobr-v2-build-state', raw);
    }, workspaceEnvelope(workspace));
    await page.goto('/build');
    await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
    await expect(busy(page)).toHaveCount(0);
    const snapshot = () => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!));
    const compare = async () => {
      await pageLink(page, 'Compare').click();
      await choose(page, compareStageA(page), 'Spark / Starter');
      await choose(page, compareStageB(page), 'Spark / Target');
      await compareTab(page, 'Config').click();
    };
    const copyUnset = async (key: string) => {
      const difference = page.locator(`[data-config-key="${key}"]`);
      await expect(difference).toContainText('Not set');
      await difference.getByRole('button', { name: 'Apply B → A', exact: true }).click();
      await expect(difference).toHaveCount(0);
    };
    const config = async () => {
      await pageLink(page, 'Config').click();
      await expect(page.getByRole('combobox', { name: 'Config set', exact: true })).toHaveText('Imported', { timeout: 90_000 });
      await expect(busy(page)).toHaveCount(0);
    };
    const row = (key: string) => page.locator(`.config-item[data-config-var="${key}"]`);
    // The row's help also carries a "source" note ("Internal key"); the value source is the one that names the saved value.
    const source = (key: string) => row(key).locator('.config-source').filter({ hasText: /saved value/i });
    const findControl = async (key: string) => {
      await page.getByRole('button', { name: 'Show all options', exact: true }).click();
      await page.getByLabel('Search config options…').fill(key);
      await expect(row(key).locator('input')).toBeEnabled();
    };
    await compare();
    await copyUnset(shockKey);
    await copyUnset(questKey);
    const copied = await snapshot();
    expect(copied.workspace.builds[0].activeStage).toBe(activeStage);
    expect(copied.workspace.builds[0].stages[1]).toEqual(workspace.builds[0].stages[1]);
    expect(copied.workspace.builds[0].stages[0].saved.state.params.config_inputs)
      .toEqual({ conditionFullLife: false, ActiveBrands: 0, futureInput: '20' });
    if (activeStage !== 'stage-a') await choose(page, page.getByRole('combobox', { name: 'Current stage', exact: true }), 'Starter');
    await config();
    await findControl(shockKey);
    await expect(row(shockKey).locator('input')).not.toBeChecked();
    await expect(source(shockKey)).toHaveText('Default · no saved value');
    await expect(row(shockKey).locator('.config-reset')).toBeVisible();
    await expect(page.locator(`.config-quick-entry[data-config-var="${shockKey}"]`)).toContainText('Catalog default');
    await expect(page.locator(`.config-quick-entry[data-config-var="${shockKey}"] output`)).toContainText('false');
    await page.getByRole('checkbox', { name: /^Configured only/ }).check();
    await expect(row(shockKey)).toHaveCount(0);
    await page.getByRole('checkbox', { name: /^Configured only/ }).uncheck();
    // Unset must resolve a true catalog default as well as shock's false default.
    await findControl(questKey);
    await expect(row(questKey).locator('input')).toBeChecked();
    await expect(source(questKey)).toHaveText('Default · no saved value');
    await expect(row(questKey).locator('.config-reset')).toBeVisible();
    await page.reload();
    await config();
    await findControl(shockKey);
    await expect(row(shockKey).locator('input')).not.toBeChecked();
    expect((await snapshot()).state.params.config_inputs).not.toHaveProperty(shockKey);
    // Reset is still relative to A's imported config, not the copied value.
    await row(shockKey).locator('.config-reset').click();
    await expect(row(shockKey).locator('input')).toBeChecked();
    await expect(row(shockKey).locator('.config-reset')).toHaveCount(0);
    expect((await snapshot()).state.params.config_inputs[shockKey]).toBe(true);
    await findControl(questKey);
    await row(questKey).locator('.config-reset').click();
    await expect(row(questKey).locator('input')).not.toBeChecked();
    await expect(row(questKey).locator('.config-reset')).toHaveCount(0);
    expect((await snapshot()).state.params.config_inputs[questKey]).toBe(false);
    await compare();
    await copyUnset(shockKey);
    await copyUnset(questKey);
    await config();
    const sets = page.getByRole('combobox', { name: 'Config set', exact: true });
    // Saving and leaving a named set makes its cleared values the new baseline.
    await choose(page, sets, 'Other');
    await expect(sets).toHaveText('Other');
    await expect(sets).toBeEnabled();
    await findControl(shockKey);
    await expect(row(shockKey).locator('input')).not.toBeChecked();
    await expect(row(shockKey).locator('.config-reset')).toHaveCount(0);
    expect((await snapshot()).state.params.config_inputs[shockKey]).toBe(false);
    await choose(page, sets, 'Imported');
    await expect(sets).toHaveText('Imported');
    await expect(sets).toBeEnabled();
    await expect(row(shockKey).locator('input')).not.toBeChecked();
    await expect(row(shockKey).locator('.config-reset')).toHaveCount(0);
    const selected = await snapshot();
    // Encoding a named set may add native defaults; the edited inputs retain
    // their types, and neither explicitly cleared key may reappear.
    expect(selected.state.params.config_inputs)
      .toMatchObject({ conditionFullLife: false, ActiveBrands: 0, futureInput: '20' });
    expect(selected.state.params.config_inputs).not.toHaveProperty(shockKey);
    expect(selected.state.params.config_inputs).not.toHaveProperty(questKey);
    expect(selected.workspace.builds[0].stages[1]).toEqual(workspace.builds[0].stages[1]);
    await row(shockKey).locator('input').check();
    await expect(row(shockKey).locator('.config-reset')).toBeEnabled();
    await row(shockKey).locator('.config-reset').click();
    await expect(row(shockKey).locator('input')).not.toBeChecked();
    expect((await snapshot()).state.params.config_inputs).not.toHaveProperty(shockKey);
    await page.reload();
    await config();
    await findControl(shockKey);
    await expect(row(shockKey).locator('input')).not.toBeChecked();
    await expect(row(shockKey).locator('.config-reset')).toHaveCount(0);
    const reloaded = await snapshot();
    expect(reloaded.state.params.config_inputs)
      .toMatchObject({ conditionFullLife: false, ActiveBrands: 0, futureInput: '20' });
    expect(reloaded.state.params.config_inputs).not.toHaveProperty(shockKey);
    expect(reloaded.state.params.config_inputs).not.toHaveProperty(questKey);
    expect(reloaded.workspace.builds[0].stages[1]).toEqual(workspace.builds[0].stages[1]);
  });
}
