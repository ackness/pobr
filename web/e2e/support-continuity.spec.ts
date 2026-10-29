import { expect, test, type Page } from '@playwright/test';
import { openItemsAfterImport } from './shell';
import { copyCurrentStage, currentStage, dpsValue, ehpValue, goTo, openCustomModifiers, openOptimizerSettings, openStage, reduceMotion, settledNumber, settledText } from './upgradesPage';
import type { SocketGroupInput } from '../src/api/types';

const dps = dpsValue;
const capacity = (page: Page) => page.getByRole('spinbutton', { name: 'Unlocked support sockets', exact: true });
const lineage = (page: Page) => page.getByRole('checkbox', { name: 'Include lineage supports', exact: true });
const run = (page: Page) => page.getByRole('button', { name: 'Calculate support combinations', exact: true });

async function syntheticCaster(page: Page) {
  // Synthetic WeGame import only: compatibility, previews and build recalculation
  // use the application's actual WASM backend and its unmodified game data.
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: 85, class_name: 'Sorceress' },
    equipments: [{ inventoryId: 'Ring', x: 0, baseType: 'Sapphire Ring', name: 'Synthetic Continuity Ring', frameType: 2,
      explicitMods: ['40% increased Fire Damage', '+80 to maximum Life'] }],
    talent_tree: { hashes: [54447, 4739, 22419, 18407], quest_stats: [] }, jewel_data: '[]',
    skills: [{ baseType: 'Fireball', support: false, properties: [{ type: 5, values: [['16', 0]] }] }],
  } }));
  await page.goto('/build');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code', exact: true })
    .fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticSupportContinuityFixture');
  await page.locator('.import-submit').click();
  await openItemsAfterImport(page);
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 30_000 });
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
}

/**
 * A stage or skill-set change gives the Skills page a new planner. The page opens on the main group with its
 * optimizer drawn, but nothing of the earlier planner is carried over: no plans, and the settings are folded again.
 */
async function expectFreshOptimizer(page: Page) {
  await expect(page.locator('.gem-optimizer-body')).toBeVisible();
  await expect(page.locator('.gem-optimizer-settings')).toHaveJSProperty('open', false);
  await expect(page.locator('.support-results')).toHaveCount(0);
  await expect(page.locator('.gem-optimizer').getByRole('button', { name: 'Apply', exact: true })).toHaveCount(0);
  await expect(page.locator('.opt-progress')).toHaveCount(0);
}

async function openOptimizer(page: Page, index = 0) {
  await goTo(page, 'Skills');
  await expect(page.locator('.skills-toolbar input')).toBeEnabled();
  const group = page.locator('.skill-group').nth(index);
  const main = group.locator('.skill-main-toggle');
  if (await main.getAttribute('aria-pressed') === 'false') {
    await main.click();
    await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  }
  const title = group.locator('.skill-group-title');
  if (await title.getAttribute('aria-expanded') === 'false') await title.click();
  const toggle = group.locator('.gem-optimizer-toggle');
  if (await toggle.getAttribute('aria-expanded') === 'false') await toggle.click();
  await expect(group.locator('.gem-optimizer-body')).toBeVisible();
  await openOptimizerSettings(group);
  await expect(run(page)).toBeEnabled();
}

/** Come back to a Skills page whose optimizer was open: the optimizer is drawn afresh, with its settings folded. */
async function returnToSkills(page: Page, options?: { force?: boolean }) {
  await goTo(page, 'Skills', options);
  await expect(page.locator('.gem-optimizer-body')).toBeVisible();
  await openOptimizerSettings(page);
}

async function completePlan(page: Page) {
  await run(page).click();
  await expect(page.locator('.support-plan').first()).toBeVisible({ timeout: 90_000 });
  await expect(run(page)).toBeEnabled();
  await expect(page.locator('.opt-progress')).toHaveCount(0);
}

const numeric = (text: string) => Number(text.replaceAll(',', ''));

async function socketGroups(page: Page): Promise<SocketGroupInput[]> {
  return page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state.socketGroups);
}

async function selectObjective(page: Page, name: string) {
  await page.getByRole('combobox', { name: 'Objective', exact: true }).click();
  await page.getByRole('option', { name, exact: true }).click();
}

async function expectNoPlans(page: Page) {
  await expect(page.locator('.support-results')).toHaveCount(0);
  await expect(page.locator('.gem-optimizer').getByRole('button', { name: 'Apply', exact: true })).toHaveCount(0);
  await expect(page.locator('.opt-progress')).toHaveCount(0);
  await expect(run(page)).toBeEnabled();
}

async function previewMetric(page: Page, label: 'DPS' | 'EHP', index = 0) {
  const text = await page.locator('.support-plan').nth(index).locator('.support-plan-metrics > span')
    .filter({ hasText: new RegExp(`^${label} `) }).innerText();
  const value = text.match(new RegExp(`^${label} ([\\d,.]+)`));
  expect(value, `The recommendation must display a numeric ${label} preview`).not.toBeNull();
  return numeric(value![1]);
}

async function addGroup(page: Page, name: string) {
  await page.getByPlaceholder('Search an active gem to add a group…').fill(name);
  await page.getByRole('option').filter({ has: page.getByText(name, { exact: true }) }).click();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
}

async function pauseAfterRealBatch(page: Page) {
  // Only suspend the existing inter-batch scheduling yield after initialization.
  // The identity and candidate probes still execute in the real WASM engine.
  await page.clock.install({ time: new Date('2026-01-01T00:00:00Z') });
  await page.clock.pauseAt(new Date('2026-01-01T00:00:01Z'));
  await run(page).click({ force: true });
  const progress = page.locator('.opt-progress progress');
  await expect.poll(async () => {
    await page.clock.runFor(1);
    return Number(await progress.getAttribute('value'));
  }, { timeout: 90_000, intervals: [10] }).toBeGreaterThan(0);
  expect(Number(await progress.getAttribute('value')),
    'Suspend an actually evaluated but incomplete search, not initialization or a completed run')
    .toBeLessThan(Number(await progress.getAttribute('max')));
  await expect(page.getByRole('button', { name: 'Cancel', exact: true })).toBeVisible();
  await expect(page.locator('.support-results')).toHaveCount(0);
}

test('completed support plans and nondefault socket and lineage settings survive Skills → Calcs → Skills', async ({ page }) => {
  await syntheticCaster(page);
  await openOptimizer(page);
  await capacity(page).fill('3');
  await lineage(page).uncheck();
  await completePlan(page);
  const beforeDps = await settledText(dps(page));
  const completed = await page.locator('.support-results').innerText();
  const recommended = await page.locator('.support-plan').first().locator('.support-plan-metrics').innerText();
  await expect(page.locator('.support-plan .gem-lineage-badge')).toHaveCount(0);

  await goTo(page, 'Calcs');
  await expect(page.getByRole('heading', { name: 'Socket Groups', exact: true })).toBeHidden();
  await expect(run(page)).toHaveCount(0);
  await expect(capacity(page)).toHaveCount(0);
  await expect(lineage(page)).toHaveCount(0);
  await expect(page.locator('.gem-optimizer').getByRole('button', { name: 'Apply', exact: true })).toHaveCount(0);

  await returnToSkills(page);
  await expect(page.locator('.skill-group-title').first()).toHaveAttribute('aria-expanded', 'true');
  await expect(page.locator('.gem-optimizer-toggle')).toHaveAttribute('aria-expanded', 'true');
  await expect(capacity(page)).toHaveValue('3');
  await expect(lineage(page)).not.toBeChecked();
  await expect.poll(() => page.locator('.support-results').innerText()).toBe(completed);
  await expect.poll(() => page.locator('.support-plan').first().locator('.support-plan-metrics').innerText()).toBe(recommended);
  await expect(page.locator('.support-plan').first().getByRole('button', { name: 'Apply', exact: true })).toBeEnabled();
  await expect(dps(page)).toHaveText(beforeDps);
});

test('same-stage Apply after returning to Skills equips the displayed complete support set and DPS preview', async ({ page }) => {
  await syntheticCaster(page);
  await openOptimizer(page);
  await capacity(page).fill('3');
  await lineage(page).uncheck();
  await selectObjective(page, 'Max total DPS');
  const groupsBefore = await socketGroups(page);
  const beforeDps = await settledNumber(dps(page));
  await completePlan(page);
  const first = page.locator('.support-plan').first();
  const ids = await first.locator('.support-plan-gems > .opt-chip').evaluateAll(elements =>
    elements.map(element => element.getAttribute('data-skill-id')!));
  expect(ids.length, 'The recommendation must fit the explicitly unlocked capacity').toBeLessThanOrEqual(3);
  await expect(first.locator('.gem-lineage-badge')).toHaveCount(0);
  const previewDps = await previewMetric(page, 'DPS');
  expect(previewDps).toBeGreaterThan(beforeDps);

  await goTo(page, 'Calcs');
  await expect(page.locator('.gem-optimizer').getByRole('button', { name: 'Apply', exact: true })).toHaveCount(0);
  await returnToSkills(page);
  await expect(first).toBeVisible();
  await first.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect.poll(async () => numeric(await dps(page).innerText())).toBeGreaterThan(beforeDps);
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  const applied = await socketGroups(page);
  expect(applied[0].gems[0]).toEqual(groupsBefore[0].gems[0]);
  expect(applied[0].gems.slice(1).map(gem => gem.skill_id)).toEqual(ids);
  expect(Math.abs(await settledNumber(dps(page)) - previewDps),
    'Equipped DPS must match the real-WASM preview to its displayed one-decimal precision').toBeLessThanOrEqual(0.06);
  await expectNoPlans(page);
  await expect(capacity(page)).toHaveValue('3');
  await expect(lineage(page)).not.toBeChecked();
});

for (const edit of ['build', 'goal'] as const) {
  test(`completed support plans invalidate after a hidden ${edit} edit without losing planner preferences`, async ({ page }) => {
    await syntheticCaster(page);
    await openOptimizer(page);
    await capacity(page).fill('3');
    await lineage(page).uncheck();
    await completePlan(page);
    const groupsBefore = await socketGroups(page);
    const originalDps = await settledText(dps(page));
    const floor = Math.max(0, await settledNumber(ehpValue(page)) - 1);

    if (edit === 'build') {
      await goTo(page, 'Config');
      await openCustomModifiers(page);
      await page.locator('.config-modifier-add').click();
      const block = page.locator('.config-modifier-block').last();
      await block.locator('.config-extra-mods').fill('100% increased Fire Damage');
      await block.locator('.config-modifier-title input').click();
      await expect(dps(page)).not.toHaveText(originalDps);
      await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
    } else {
      await goTo(page, 'Upgrades');
      await page.getByRole('button', { name: 'Max total DPS', exact: true }).click();
      await page.locator('.upgrade-goal-details > summary').click();
      await page.getByRole('combobox', { name: 'Constraint', exact: true }).click();
      await page.getByRole('option', { name: 'Effective HP', exact: true }).click();
      await page.getByRole('spinbutton', { name: 'min', exact: true }).fill(String(floor));
      await expect(dps(page)).toHaveText(originalDps);
    }
    await returnToSkills(page);
    await expect(capacity(page)).toHaveValue('3');
    await expect(lineage(page)).not.toBeChecked();
    await expectNoPlans(page);
    expect(await socketGroups(page)).toEqual(groupsBefore);
    const currentDps = await settledNumber(dps(page));
    await completePlan(page);
    const baselineText = await page.locator('.support-baseline').innerText();
    const baselineDps = numeric(baselineText.match(/DPS ([\d,.]+)/)![1]);
    expect(Math.abs(baselineDps - currentDps), 'New results must use the current equipped build, not the hidden old request')
      .toBeLessThanOrEqual(0.06);
    if (edit === 'goal') {
      await expect(page.getByRole('combobox', { name: 'Objective', exact: true })).toContainText('Max total DPS');
      await expect(page.getByRole('spinbutton', { name: 'min', exact: true })).toHaveValue(String(floor));
      const planCount = await page.locator('.support-plan').count();
      for (let index = 0; index < planCount; index++) {
        expect(await previewMetric(page, 'EHP', index), 'Every refreshed recommendation must satisfy the new shared constraint')
          .toBeGreaterThanOrEqual(floor - 0.05);
      }
    }
    expect(await socketGroups(page)).toEqual(groupsBefore);
  });
}

test('switching identical copied stages resets completed support plans and stage-local settings', async ({ page }) => {
  await syntheticCaster(page);
  await openOptimizer(page);
  const initialCapacity = await capacity(page).inputValue();
  const initialLineage = await lineage(page).isChecked();
  await capacity(page).fill('3');
  await lineage(page).uncheck();
  await completePlan(page);
  const originalGroups = await socketGroups(page);
  const originalDps = await settledText(dps(page));

  const originalStage = (await currentStage(page).innerText()).trim();
  await copyCurrentStage(page, 'Synthetic support copy');
  await goTo(page, 'Skills');
  await expectFreshOptimizer(page);
  await openOptimizer(page);
  await expectNoPlans(page);
  await expect(capacity(page)).toHaveValue(initialCapacity);
  expect(await lineage(page).isChecked()).toBe(initialLineage);
  expect(await socketGroups(page)).toEqual(originalGroups);
  await expect(dps(page)).toHaveText(originalDps);
  await capacity(page).fill('4');
  await lineage(page).uncheck();
  await completePlan(page);

  await openStage(page, originalStage);
  await goTo(page, 'Skills');
  await expectFreshOptimizer(page);
  await openOptimizer(page);
  await expectNoPlans(page);
  await expect(capacity(page)).toHaveValue(initialCapacity);
  expect(await lineage(page).isChecked()).toBe(initialLineage);
  expect(await socketGroups(page)).toEqual(originalGroups);
  await expect(dps(page)).toHaveText(originalDps);
});

test('retargeting keeps only the selected workspace and never borrows another group settings or preview', async ({ page }) => {
  await syntheticCaster(page);
  await openOptimizer(page);
  const initialCapacity = await capacity(page).inputValue();
  const initialLineage = await lineage(page).isChecked();
  await capacity(page).fill('3');
  await lineage(page).uncheck();
  await completePlan(page);
  const originalGroups = await socketGroups(page);

  await addGroup(page, 'Spark');
  await openOptimizer(page, 1);
  await expect(capacity(page)).toHaveValue(initialCapacity);
  expect(await lineage(page).isChecked()).toBe(initialLineage);
  await expectNoPlans(page);
  await capacity(page).fill('4');
  await completePlan(page);
  const sparkPreview = await page.locator('.skill-group').nth(1).locator('.support-results').innerText();
  await expect(page.locator('.skill-group').nth(1).locator('.skill-main-toggle')).toHaveAttribute('aria-pressed', 'true');

  await openOptimizer(page, 0);
  await expect(capacity(page)).toHaveValue(initialCapacity);
  expect(await lineage(page).isChecked()).toBe(initialLineage);
  await expect(page.locator('.skill-group').first().locator('.skill-main-toggle')).toHaveAttribute('aria-pressed', 'true');
  await expectNoPlans(page);
  await completePlan(page);
  expect(await page.locator('.skill-group').first().locator('.support-results').innerText()).not.toBe(sparkPreview);
  expect((await socketGroups(page))[0]).toEqual(originalGroups[0]);

  await openOptimizer(page, 1);
  await expect(capacity(page)).toHaveValue(initialCapacity);
  expect(await lineage(page).isChecked()).toBe(initialLineage);
  await expectNoPlans(page);
});

test('deleting a preceding duplicate skill group resets planner ownership instead of transferring its settings or plans', async ({ page }) => {
  await syntheticCaster(page);
  await openOptimizer(page);
  const initialCapacity = await capacity(page).inputValue();
  const initialLineage = await lineage(page).isChecked();
  await capacity(page).fill('3');
  await lineage(page).uncheck();
  await completePlan(page);
  await addGroup(page, 'Fireball');
  await openOptimizer(page, 1);
  await expect(capacity(page)).toHaveValue(initialCapacity);
  expect(await lineage(page).isChecked()).toBe(initialLineage);
  await expectNoPlans(page);
  await capacity(page).fill('4');
  await completePlan(page);
  const surviving = (await socketGroups(page))[1];

  await page.locator('.skill-group').first().getByRole('button', { name: 'Remove group', exact: true }).click();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await openOptimizer(page);
  await expect(page.locator('.skill-group')).toHaveCount(1);
  await expectNoPlans(page);
  await expect(capacity(page)).toHaveValue(initialCapacity);
  expect(await lineage(page).isChecked()).toBe(initialLineage);
  expect(await socketGroups(page)).toEqual([surviving]);
  await expect(page.locator('.skill-main-toggle')).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('.skill-gems').getByRole('spinbutton', { name: 'Level', exact: true })).toHaveValue(String(surviving.gems[0].level));
});

test('Cancel discards evaluated partial support plans and an explicit fresh run remains usable', async ({ page }) => {
  await reduceMotion(page); // the clock is frozen below
  await syntheticCaster(page);
  await openOptimizer(page);
  await capacity(page).fill('3');
  await lineage(page).uncheck();
  const equipped = await socketGroups(page);
  const beforeDps = await settledText(dps(page));
  await pauseAfterRealBatch(page);
  await page.getByRole('button', { name: 'Cancel', exact: true }).click({ force: true });
  await page.clock.runFor(1000);
  await expectNoPlans(page);
  expect(await socketGroups(page)).toEqual(equipped);
  await expect(dps(page)).toHaveText(beforeDps);
  await expect(capacity(page)).toHaveValue('3');
  await expect(lineage(page)).not.toBeChecked();
  await page.clock.resume();
  await completePlan(page);
  expect(await previewMetric(page, 'DPS')).toBeGreaterThan(numeric(beforeDps));
  expect(await socketGroups(page)).toEqual(equipped);
});

test('leaving Skills cancels an active real-WASM batch and its late continuation cannot publish hidden support plans', async ({ page }) => {
  await reduceMotion(page); // the clock is frozen below
  await syntheticCaster(page);
  await openOptimizer(page);
  await capacity(page).fill('3');
  await lineage(page).uncheck();
  const equipped = await socketGroups(page);
  const beforeDps = await settledText(dps(page));
  await pauseAfterRealBatch(page);
  await goTo(page, 'Calcs', { force: true });
  await expect(page.getByRole('button', { name: 'Cancel', exact: true })).toHaveCount(0);
  await expect(run(page)).toHaveCount(0);
  await expect(capacity(page)).toHaveCount(0);
  await expect(page.locator('.gem-optimizer').getByRole('button', { name: 'Apply', exact: true })).toHaveCount(0);
  // Drain the response-processing yield that already follows real candidate work.
  await page.clock.runFor(1000);
  await expect(page.locator('.support-results')).toHaveCount(0);
  await expect(page.locator('.opt-progress')).toHaveCount(0);
  expect(await socketGroups(page)).toEqual(equipped);
  await expect(dps(page)).toHaveText(beforeDps);
  await returnToSkills(page, { force: true });
  await expectNoPlans(page);
  await expect(capacity(page)).toHaveValue('3');
  await expect(lineage(page)).not.toBeChecked();
  await page.clock.resume();
  await completePlan(page);
  expect(await previewMetric(page, 'DPS')).toBeGreaterThan(numeric(beforeDps));
  expect(await socketGroups(page)).toEqual(equipped);
});

test('a hidden shared-goal edit cannot be overwritten by the previous real support batch continuation', async ({ page }) => {
  await reduceMotion(page); // the clock is frozen below
  await syntheticCaster(page);
  await openOptimizer(page);
  await capacity(page).fill('3');
  await lineage(page).uncheck();
  const equipped = await socketGroups(page);
  const beforeDps = await settledText(dps(page));
  await pauseAfterRealBatch(page);
  await goTo(page, 'Upgrades', { force: true });
  await page.getByRole('button', { name: 'Max total DPS', exact: true }).click({ force: true });
  await page.getByRole('checkbox', { name: 'Prioritize capped elemental resistances', exact: true }).check({ force: true });
  await page.clock.runFor(1000);
  await expect(page.locator('.support-results')).toHaveCount(0);
  await expect(page.locator('.opt-progress')).toHaveCount(0);
  await returnToSkills(page, { force: true });
  await expectNoPlans(page);
  await expect(page.getByRole('combobox', { name: 'Objective', exact: true })).toContainText('Max total DPS');
  await expect(page.getByRole('checkbox', { name: 'Prioritize capped elemental resistances', exact: true })).toBeChecked();
  await expect(capacity(page)).toHaveValue('3');
  await expect(lineage(page)).not.toBeChecked();
  expect(await socketGroups(page)).toEqual(equipped);
  await expect(dps(page)).toHaveText(beforeDps);
  await page.clock.resume();
  await completePlan(page);
  expect(await socketGroups(page)).toEqual(equipped);
});

test('replacing a skill set with identical groups resets selected support workspace ownership', async ({ page }) => {
  await syntheticCaster(page);
  await openOptimizer(page);
  const initialCapacity = await capacity(page).inputValue();
  const initialLineage = await lineage(page).isChecked();
  const groups = await socketGroups(page);
  await page.getByRole('textbox', { name: 'Set name…', exact: true }).fill('Identical support set');
  await page.getByRole('button', { name: 'Save current as set', exact: true }).click();
  await capacity(page).fill('3');
  await lineage(page).uncheck();
  await completePlan(page);
  await page.locator('.skill-set-chip').filter({ hasText: 'Identical support set' })
    .getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await expectFreshOptimizer(page);
  expect(await socketGroups(page)).toEqual(groups);
  await openOptimizer(page);
  await expectNoPlans(page);
  await expect(capacity(page)).toHaveValue(initialCapacity);
  expect(await lineage(page).isChecked()).toBe(initialLineage);
});

test('stage switching in Skills does not replay a prior-stage focus but accepts a new planning action', async ({ page }) => {
  await syntheticCaster(page);
  const originalStage = (await currentStage(page).innerText()).trim();
  await copyCurrentStage(page, 'Focus copy');
  await goTo(page, 'Upgrades');
  await page.locator('.upgrade-paths > button').nth(1).click();
  await expect(page.locator('.gem-optimizer-body')).toBeVisible();
  await openStage(page, originalStage);
  await expectFreshOptimizer(page);
  await goTo(page, 'Upgrades');
  await page.locator('.upgrade-paths > button').nth(1).click();
  await expect(page.locator('.gem-optimizer-body')).toBeVisible();
});
