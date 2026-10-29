import { expect, test, type Page } from '@playwright/test';
import { bdLibraryButton, statValue } from './shell';
import { choose, goTo } from './skillsAndCalcs';
import type { GemInput, SocketGroupInput } from '../src/api/types';

// A charm line the engine does not calculate: it shows as a limit of the baseline. ("Also grants 102 Guard",
// used here before, is calculated by now.)
const limitLine = 'Remove Bleeding on use';
const run = (page: Page) => page.getByRole('button', { name: 'Calculate support combinations', exact: true });
const capacity = (page: Page) => page.getByRole('spinbutton', { name: 'Unlocked support sockets', exact: true });
const baseline = (page: Page) => page.locator('details.support-baseline-unsupported');
const dps = (page: Page) => statValue(page, /^Total DPS$/).first();
const numeric = (text: string) => Number(text.replaceAll(',', ''));

async function groups(page: Page): Promise<SocketGroupInput[]> {
  return page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state.socketGroups);
}

async function casterWithLimit(page: Page) {
  // Only import/league HTTP responses are synthetic. All compatibility, diagnostics,
  // search previews and applied calculations come from the unmodified real WASM.
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: 85, class_name: 'Sorceress' },
    equipments: [
      { inventoryId: 'Ring', x: 0, baseType: 'Sapphire Ring', name: 'Synthetic Diagnostic Ring', frameType: 2,
        explicitMods: ['40% increased Fire Damage', '+80 to maximum Life'] },
      ...['Ultimate Mana Flask', 'Ultimate Life Flask', 'Thawing Charm', 'Stone Charm', 'Silver Charm']
        .map((baseType, x) => ({ inventoryId: 'Flask', x, baseType, name: 'Synthetic Diagnostic Utility', frameType: 1,
          explicitMods: x === 3 ? [limitLine] : [] })),
    ],
    talent_tree: { hashes: [54447, 4739, 22419, 18407], quest_stats: [] }, jewel_data: '[]',
    skills: [{ baseType: 'Fireball', support: false, properties: [{ type: 5, values: [['16', 0]] }] }],
  } }));
  await page.goto('/build');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code', exact: true })
    .fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticSupportDiagnostics');
  await page.locator('.import-submit').click();
  await expect(page.locator('#ov-title')).toBeVisible({ timeout: 90_000 });
  await goTo(page, 'Items');
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 30_000 });
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  // The Overview lists the modifier lines the calculation does not apply.
  await goTo(page, 'Overview');
  const limits = page.locator('.ov-details');
  await limits.locator('summary').click();
  await expect(limits.locator('li')).toHaveText([limitLine]);
  await openOptimizer(page);
}

async function openOptimizer(page: Page) {
  await goTo(page, 'Skills');
  await expect(page.locator('.skills-toolbar input')).toBeEnabled();
  const group = page.locator('.skill-group').first();
  if (await group.locator('.skill-main-toggle').getAttribute('aria-pressed') === 'false') {
    await group.locator('.skill-main-toggle').click();
    await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  }
  if (await group.locator('.skill-group-title').getAttribute('aria-expanded') === 'false') {
    await group.locator('.skill-group-title').click();
  }
  if (await group.locator('.gem-optimizer-toggle').getAttribute('aria-expanded') === 'false') {
    await group.locator('.gem-optimizer-toggle').click();
  }
  // Capacity, objective and the candidate pool sit under the folded "Goal, capacity and candidates".
  const settings = page.locator('.gem-optimizer-settings');
  await expect(settings).toBeVisible();
  if (!(await settings.evaluate(element => (element as HTMLDetailsElement).open))) await settings.locator(':scope > summary').click();
  await expect(run(page)).toBeEnabled();
}

async function completeSearch(page: Page) {
  await run(page).click();
  await expect(page.locator('.support-plan').first()).toBeVisible({ timeout: 90_000 });
  await expect(run(page)).toBeEnabled();
  await expect(page.locator('.opt-progress')).toHaveCount(0);
  await expect(baseline(page)).toBeVisible();
}

async function expectInvalidated(page: Page) {
  await expect(page.locator('.support-results')).toHaveCount(0);
  await expect(baseline(page)).toHaveCount(0);
  await expect(page.locator('.support-diagnostic-excluded')).toHaveCount(0);
  await expect(page.locator('.gem-optimizer').getByRole('button', { name: 'Apply', exact: true })).toHaveCount(0);
}

test('real baseline limits remain keyboard-readable across first page visits and Apply uses the exact recommended set', async ({ page }) => {
  await casterWithLimit(page);
  const before = await groups(page);
  await capacity(page).fill('2');
  await page.getByRole('checkbox', { name: 'Include lineage supports', exact: true }).uncheck();
  await choose(page, page.getByRole('combobox', { name: 'Objective', exact: true }), 'Max total DPS');
  await completeSearch(page);

  const summary = baseline(page).locator('summary');
  await summary.focus();
  await page.keyboard.press('Enter');
  await expect(baseline(page)).toHaveAttribute('open', '');
  await expect(baseline(page).locator('ul > li')).toHaveText([limitLine]);
  await page.keyboard.press('Space');
  await expect(baseline(page)).not.toHaveAttribute('open', '');
  await page.keyboard.press('Enter');
  await expect(baseline(page).locator('ul > li')).toBeVisible();

  // Some real builds have no rejected sets. Do not invent engine failures to fill
  // this list; policy/provenance edge cases live at the injected unit-test seam.
  const rejected = page.locator('ol.support-diagnostic-excluded');
  if (await rejected.count()) {
    await rejected.locator('..').locator(':scope > summary').click();
    const entries = rejected.locator(':scope > li > details');
    for (let index = 0; index < await entries.count(); index++) {
      await entries.nth(index).locator('summary').click();
      await expect(entries.nth(index).getByRole('button', { name: 'Apply', exact: true })).toHaveCount(0);
      await expect(entries.nth(index).locator('.support-plan')).toHaveCount(0);
    }
  }
  const raw = await baseline(page).locator('ul > li').allTextContents();
  const recommendations = await page.locator('.support-plan').allTextContents();
  const first = page.locator('.support-plan').first();
  const expected: GemInput[] = await first.locator('.support-plan-gems > .opt-chip').evaluateAll(chips => chips.map(chip => {
    const statSet = chip.getAttribute('data-stat-set-index');
    return { skill_id: chip.getAttribute('data-skill-id')!, level: Number(chip.getAttribute('data-level')),
      quality: Number(chip.getAttribute('data-quality')), stat_set_index: statSet === null ? 1 : Number(statSet) };
  }));
  expect(expected.every(gem => gem.level > 0 && gem.quality >= 0)).toBe(true);
  const metric = await first.locator('.support-plan-metrics > span').filter({ hasText: /^DPS / }).innerText();
  const preview = numeric(metric.match(/^DPS ([\d,.]+)/)![1]);

  for (const name of ['Calcs', 'Tree', 'Upgrades']) {
    await goTo(page, name);
    await expect(page.locator(name === 'Calcs' ? '.calcs-page' : name === 'Tree' ? '.tree-canvas' : '.upgrade-paths'))
      .toBeVisible();
    await expect(run(page)).toHaveCount(0);
    await goTo(page, 'Skills');
    await expect(baseline(page).locator('ul > li')).toHaveText(raw);
    await expect.poll(() => page.locator('.support-plan').allTextContents()).toEqual(recommendations);
    await expect(first.getByRole('button', { name: 'Apply', exact: true })).toBeEnabled();
  }
  await first.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await expect.poll(async () => {
    const applied = (await groups(page))[0].gems;
    return [applied[0], ...applied.slice(1).map(gem => ({ ...gem, stat_set_index: gem.stat_set_index ?? 1 }))];
  }).toEqual([before[0].gems[0], ...expected]);
  expect((await groups(page)).slice(1)).toEqual(before.slice(1));
  await expect.poll(async () => Math.abs(numeric(await dps(page).innerText()) - preview)).toBeLessThanOrEqual(0.06);
  await expectInvalidated(page);
});

test('diagnostic receipts invalidate on search input edits and identical copied-stage switches', async ({ page }) => {
  await casterWithLimit(page);
  await capacity(page).fill('2');
  await page.getByRole('checkbox', { name: 'Include lineage supports', exact: true }).uncheck();
  await completeSearch(page);
  await baseline(page).locator('summary').click();
  await expect(baseline(page).locator('ul > li')).toHaveText([limitLine]);
  await capacity(page).fill('3');
  await expectInvalidated(page);
  await capacity(page).fill('2');
  await completeSearch(page);
  const before = await groups(page);
  const stageSwitch = page.getByRole('combobox', { name: 'Current stage', exact: true });
  const originalStage = (await stageSwitch.innerText()).trim();
  await bdLibraryButton(page).click();
  await page.getByRole('button', { name: 'Copy current stage', exact: true }).click();
  await page.getByRole('textbox', { name: 'Name', exact: true }).fill('Synthetic diagnostic copy');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await openOptimizer(page);
  await expectInvalidated(page);
  expect(await groups(page)).toEqual(before);
  await choose(page, stageSwitch, originalStage);
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await openOptimizer(page);
  await expectInvalidated(page);
  expect(await groups(page)).toEqual(before);
});
