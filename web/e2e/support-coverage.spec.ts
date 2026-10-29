import { expect, test, type Locator, type Page } from '@playwright/test';
import { statValue } from './shell';
import { choose, goTo } from './skillsAndCalcs';
import type { GemInput, SocketGroupInput } from '../src/api/types';

// Swift Affliction III still has an unmodelled conditional duration threshold.
const swift = 'SupportSwiftAfflictionPlayerThree';
const swiftStat = 'support_swift_affliction_non_ailment_damage_over_time_+%_final_if_duration_below_1_second';
const modeled = ['SupportControlledDestructionPlayer', 'SupportFireMasteryPlayer'];
const run = (page: Page) => page.getByRole('button', { name: 'Calculate support combinations', exact: true });
const baseline = (page: Page) => page.locator('details.support-baseline-unsupported');
const dps = (page: Page) => statValue(page, /^Total DPS$/).first();
// The Overview lists the modifier lines the calculation does not apply.
const unsupportedLines = (page: Page) => page.locator('.ov-details');
const numeric = (text: string) => Number(text.replaceAll(',', ''));

async function groups(page: Page): Promise<SocketGroupInput[]> {
  return page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state.socketGroups);
}

async function rejectedReceipt(rows: Locator) {
  return rows.locator(':scope > li > details').evaluateAll(edits => edits.map(edit => ({
    gems: [...edit.querySelectorAll('.support-diagnostic-gems [data-skill-id]')].map(gem => ({
      id: gem.getAttribute('data-skill-id'), level: gem.getAttribute('data-level'),
      quality: gem.getAttribute('data-quality'), set: gem.getAttribute('data-stat-set-index'),
    })),
    reasons: [...edit.querySelectorAll(':scope > ul:not(.support-diagnostic-gems) > li')].map(reason => reason.textContent),
  })));
}

async function recommendationsReceipt(page: Page) {
  return page.locator('.support-plan').evaluateAll(plans => plans.map(plan => ({
    gems: [...plan.querySelectorAll('.support-plan-gems > .opt-chip')].map(gem => ({
      id: gem.getAttribute('data-skill-id'), level: gem.getAttribute('data-level'),
      quality: gem.getAttribute('data-quality'), set: gem.getAttribute('data-stat-set-index'),
    })),
    metrics: [...plan.querySelectorAll('.support-plan-metrics > span')].map(metric =>
      [...(metric.textContent ?? '').matchAll(/[-+]?[\d,]+(?:\.\d+)?/g)].map(match => Number(match[0].replaceAll(',', '')))),
  })));
}

function expectSwiftIdentity(text: string) {
  for (const token of [`Support:${swift}:`, 'set=1:', `stat=${swiftStat}:`, 'value=50:', 'host=FireballPlayer:', 'host_set=1:']) {
    expect(text).toContain(token);
  }
}

async function syntheticCaster(page: Page, equippedSwift: boolean) {
  // These HTTP import/league fixtures are explicitly synthetic. Data, compatibility,
  // diagnostics, candidate calculations and Apply use the application's real WASM.
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: 85, class_name: 'Sorceress' },
    equipments: [{ inventoryId: 'Ring', x: 0, baseType: 'Sapphire Ring', name: 'Synthetic Coverage Ring', frameType: 2,
      explicitMods: ['40% increased Fire Damage', '+80 to maximum Life'] }],
    talent_tree: { hashes: [54447, 4739, 22419, 18407], quest_stats: [] }, jewel_data: '[]',
    skills: [{ baseType: 'Fireball', support: false, properties: [{ type: 5, values: [['16', 0]] }],
      socketedItems: equippedSwift ? [{ baseType: 'Swift Affliction III', support: true, properties: [{ type: 5, values: [['1', 0]] }] }] : [] }],
  } }));
  await page.goto('/build');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code', exact: true })
    .fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticSupportCoverageFixture');
  await page.locator('.import-submit').click();
  await expect(page.locator('#ov-title')).toBeVisible({ timeout: 90_000 });
  await goTo(page, 'Items');
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 30_000 });
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  expect((await groups(page))[0].gems.map(gem => gem.skill_id))
    .toEqual(['FireballPlayer', ...(equippedSwift ? [swift] : [])]);
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

async function boundRealCandidatePool(page: Page) {
  await page.getByRole('spinbutton', { name: 'Unlocked support sockets', exact: true }).fill('2');
  await page.getByRole('checkbox', { name: 'Include lineage supports', exact: true }).uncheck();
  await choose(page, page.getByRole('combobox', { name: 'Objective', exact: true }), 'Max total DPS');
  const pool = page.locator('.support-pool-details');
  await pool.locator('summary').click();
  for (const id of [swift, ...modeled]) {
    await expect(pool.locator(`[data-skill-id="${id}"] input`)).toBeChecked();
  }
  // Exercise the real exclusion controls to keep this a focused three-support
  // search, rather than substituting a fake catalog or candidate response.
  const candidates = pool.locator('.support-candidate');
  for (let index = 0; index < await candidates.count(); index++) {
    const candidate = candidates.nth(index);
    if (![swift, ...modeled].includes((await candidate.getAttribute('data-skill-id'))!)) {
      await candidate.locator('input').uncheck();
    }
  }
  await pool.locator('summary').click();
}

async function completeSearch(page: Page) {
  await run(page).click();
  await expect(page.locator('.support-plan').first()).toBeVisible({ timeout: 90_000 });
  await expect(run(page)).toBeEnabled();
  await expect(page.locator('.opt-progress')).toHaveCount(0);
}


async function expectInvalidated(page: Page) {
  await expect(page.locator('.support-results')).toHaveCount(0);
  await expect(baseline(page)).toHaveCount(0);
  await expect(page.locator('.support-diagnostic-excluded')).toHaveCount(0);
  await expect(page.locator('.gem-optimizer').getByRole('button', { name: 'Apply', exact: true })).toHaveCount(0);
}

for (const [layout, viewport] of [
  ['desktop', { width: 1440, height: 1000 }],
  ['narrow', { width: 390, height: 844 }],
] as const) {
  test.describe(layout, () => {
    test.use({ viewport });

    test('equipped Swift Affliction III limit stays visible and deterministic, then disappears on support removal', async ({ page }) => {
      await syntheticCaster(page, true);
      await goTo(page, 'Overview');
      const buildLimits = unsupportedLines(page);
      await expect(buildLimits).toBeVisible();
      await buildLimits.locator('summary').click();
      const normalReceipt = (await buildLimits.locator('li').filter({ hasText: swiftStat }).textContent())!;
      expectSwiftIdentity(normalReceipt);
      expect(await page.locator('main').evaluate(main => main.scrollWidth - main.clientWidth)).toBeLessThanOrEqual(1);
      await openOptimizer(page);
      await boundRealCandidatePool(page);
      await completeSearch(page);
      const summary = baseline(page).locator('summary');
      await summary.focus();
      await page.keyboard.press('Enter');
      await expect(baseline(page)).toHaveAttribute('open', '');
      await expect(baseline(page).locator('ul > li')).toHaveText([normalReceipt]);
      await page.keyboard.press('Space');
      await expect(baseline(page)).not.toHaveAttribute('open', '');
      await page.keyboard.press('Enter');
      await expect(baseline(page).locator('ul > li')).toBeVisible();
      // Baseline limits are disclosed, not newly introduced rejection reasons.
      await expect(page.locator('.support-diagnostic-excluded')).toHaveCount(0);
      const plans = await recommendationsReceipt(page);
      for (const name of ['Calcs', 'Tree']) {
        await goTo(page, name);
        await expect(page.locator(name === 'Calcs' ? '.calcs-page' : '.tree-canvas')).toBeVisible();
        await expect(run(page)).toHaveCount(0);
        await goTo(page, 'Skills');
        await expect(baseline(page).locator('ul > li')).toHaveText([normalReceipt]);
        await expect.poll(() => recommendationsReceipt(page)).toEqual(plans);
      }
      await completeSearch(page);
      await baseline(page).locator('summary').click();
      await expect(baseline(page).locator('ul > li')).toHaveText([normalReceipt]);
      const swiftGem = page.locator('.skill-gems > li').nth(1);
      await swiftGem.locator('button.skill-remove').click();
      await expect.poll(async () => (await groups(page))[0].gems.map(gem => gem.skill_id)).toEqual(['FireballPlayer']);
      await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
      await expectInvalidated(page);
      await goTo(page, 'Overview');
      await expect(unsupportedLines(page)).toHaveCount(0);
    });

    test('new Swift Affliction III complete edits are rejected without Apply while a modeled retained plan applies exactly', async ({ page }) => {
      await syntheticCaster(page, false);
      await openOptimizer(page);
      await boundRealCandidatePool(page);
      const before = await groups(page);
      const beforeDps = numeric(await dps(page).innerText());
      await completeSearch(page);
      await expect(baseline(page)).toHaveCount(0);
      const rejected = page.locator('ol.support-diagnostic-excluded');
      await expect(rejected).toHaveCount(1);
      await rejected.locator('..').locator(':scope > summary').click();
      const swiftEdits = rejected.locator(':scope > li > details')
        .filter({ has: page.locator(`.support-diagnostic-gems [data-skill-id="${swift}"]`) });
      expect(await swiftEdits.count(), 'The actual Swift Affliction III edit must be evaluated and receipted, not absent from the pool').toBeGreaterThan(0);
      const single = swiftEdits.filter({ has: page.locator('.support-diagnostic-gems > li:only-child') });
      await expect(single).toHaveCount(1);
      for (let index = 0; index < await swiftEdits.count(); index++) {
        const edit = swiftEdits.nth(index);
        await edit.locator('summary').click();
        const reasons = edit.locator(':scope > ul:not(.support-diagnostic-gems) > li');
        await expect(reasons).toHaveCount(1);
        expectSwiftIdentity(await reasons.innerText());
        await expect(edit.getByRole('button', { name: 'Apply', exact: true })).toHaveCount(0);
        await expect(edit.locator('.support-plan')).toHaveCount(0);
      }
      await expect(page.locator(`.support-plan [data-skill-id="${swift}"]`)).toHaveCount(0);
      const first = page.locator('.support-plan').first();
      const expected: GemInput[] = await first.locator('.support-plan-gems > .opt-chip').evaluateAll(chips => chips.map(chip => ({
        skill_id: chip.getAttribute('data-skill-id')!, level: Number(chip.getAttribute('data-level')),
        quality: Number(chip.getAttribute('data-quality')), stat_set_index: Number(chip.getAttribute('data-stat-set-index') ?? 1),
      })));
      expect(expected.map(gem => gem.skill_id).sort()).toEqual([...modeled].sort());
      const metric = await first.locator('.support-plan-metrics > span').filter({ hasText: /^DPS / }).innerText();
      const preview = numeric(metric.match(/^DPS ([\d,.]+)/)![1]);
      expect(preview).toBeGreaterThan(beforeDps);
      const excluded = await rejectedReceipt(rejected);
      const recommendations = await recommendationsReceipt(page);
      for (const name of ['Calcs', 'Tree', 'Upgrades']) {
        await goTo(page, name);
        await expect(page.locator(name === 'Calcs' ? '.calcs-page' : name === 'Tree' ? '.tree-canvas' : '.upgrade-paths')).toBeVisible();
        await expect(run(page)).toHaveCount(0);
        await goTo(page, 'Skills');
        await expect.poll(() => rejectedReceipt(rejected)).toEqual(excluded);
        await expect.poll(() => recommendationsReceipt(page)).toEqual(recommendations);
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
      await goTo(page, 'Overview');
      await expect(unsupportedLines(page)).toHaveCount(0);
    });
  });
}
