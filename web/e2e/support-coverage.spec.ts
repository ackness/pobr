import { expect, test, type Locator, type Page } from '@playwright/test';
import type { GemInput, SocketGroupInput } from '../src/api/types';

const fork = 'SupportForkPlayer';
const forkStat = 'support_fork_forked_projectile_damage_+%_final';
const modeled = ['SupportControlledDestructionPlayer', 'SupportFireMasteryPlayer'];
const nav = (page: Page, name: string) => page.getByRole('navigation', { name: 'Main navigation' })
  .getByRole('button', { name, exact: true });
const run = (page: Page) => page.getByRole('button', { name: 'Calculate support combinations', exact: true });
const baseline = (page: Page) => page.locator('details.support-baseline-unsupported');
const numeric = (text: string) => Number(text.replaceAll(',', ''));

async function groups(page: Page): Promise<SocketGroupInput[]> {
  return page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!).state.socketGroups);
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

function expectForkIdentity(text: string) {
  for (const token of [`Support:${fork}:`, 'set=1:', `stat=${forkStat}:`, 'value=-30:', 'host=FireballPlayer:', 'host_set=1:']) {
    expect(text).toContain(token);
  }
}

async function syntheticCaster(page: Page, equippedFork: boolean) {
  // These HTTP import/league fixtures are explicitly synthetic. Data, compatibility,
  // diagnostics, candidate calculations and Apply use the application's real WASM.
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: 85, class_name: 'Sorceress' },
    equipments: [{ inventoryId: 'Ring', x: 0, baseType: 'Sapphire Ring', name: 'Synthetic Coverage Ring', frameType: 2,
      explicitMods: ['40% increased Fire Damage', '+80 to maximum Life'] }],
    talent_tree: { hashes: [54447, 4739, 22419, 18407], quest_stats: [] }, jewel_data: '[]',
    skills: [{ baseType: 'Fireball', support: false, properties: [{ type: 5, values: [['16', 0]] }],
      socketedItems: equippedFork ? [{ baseType: 'Fork', support: true, properties: [{ type: 5, values: [['1', 0]] }] }] : [] }],
  } }));
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code', exact: true })
    .fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticSupportCoverageFixture');
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 30_000 });
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  expect((await groups(page))[0].gems.map(gem => gem.skill_id))
    .toEqual(['FireballPlayer', ...(equippedFork ? [fork] : [])]);
}

async function openOptimizer(page: Page) {
  await nav(page, 'Skills').click();
  await expect(page.locator('.skills-toolbar input')).toBeEnabled();
  const group = page.locator('.skill-group').first();
  if (await group.locator('.skill-main-toggle').getAttribute('aria-pressed') === 'false') {
    await group.locator('.skill-main-toggle').click();
    await expect(page.locator('.topbar-busy')).toHaveCount(0);
  }
  if (await group.locator('.skill-group-title').getAttribute('aria-expanded') === 'false') {
    await group.locator('.skill-group-title').click();
  }
  if (await group.locator('.gem-optimizer-toggle').getAttribute('aria-expanded') === 'false') {
    await group.locator('.gem-optimizer-toggle').click();
  }
  await expect(run(page)).toBeEnabled();
}

async function boundRealCandidatePool(page: Page) {
  await page.getByRole('spinbutton', { name: 'Unlocked support sockets', exact: true }).fill('2');
  await page.getByRole('checkbox', { name: 'Include lineage supports', exact: true }).uncheck();
  await page.getByRole('button', { name: 'Objective', exact: true }).click();
  await page.getByRole('option', { name: 'Max total DPS', exact: true }).click();
  const pool = page.locator('.support-pool-details');
  await pool.locator('summary').click();
  for (const id of [fork, ...modeled]) {
    await expect(pool.locator(`[data-skill-id="${id}"] input`)).toBeChecked();
  }
  // Exercise the real exclusion controls to keep this a focused three-support
  // search, rather than substituting a fake catalog or candidate response.
  const candidates = pool.locator('.support-candidate');
  for (let index = 0; index < await candidates.count(); index++) {
    const candidate = candidates.nth(index);
    if (![fork, ...modeled].includes((await candidate.getAttribute('data-skill-id'))!)) {
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

    test('equipped Fork limit stays visible and deterministic, then disappears on support removal', async ({ page }) => {
      await syntheticCaster(page, true);
      await nav(page, 'Build').click();
      const buildLimits = page.locator('.unsupported-block');
      await expect(buildLimits).toBeVisible();
      await buildLimits.locator('summary').click();
      const normalReceipt = (await buildLimits.locator('li').filter({ hasText: forkStat }).textContent())!;
      expectForkIdentity(normalReceipt);
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
        await nav(page, name).click();
        await expect(page.locator(name === 'Calcs' ? '.calcs-page' : '.tree-canvas')).toBeVisible();
        await expect(run(page)).toHaveCount(0);
        await nav(page, 'Skills').click();
        await expect(baseline(page).locator('ul > li')).toHaveText([normalReceipt]);
        await expect.poll(() => recommendationsReceipt(page)).toEqual(plans);
      }
      await completeSearch(page);
      await baseline(page).locator('summary').click();
      await expect(baseline(page).locator('ul > li')).toHaveText([normalReceipt]);
      const forkGem = page.locator('.skill-gems > li').nth(1);
      await forkGem.locator('button.skill-remove').click();
      await expect.poll(async () => (await groups(page))[0].gems.map(gem => gem.skill_id)).toEqual(['FireballPlayer']);
      await expect(page.locator('.topbar-busy')).toHaveCount(0);
      await expectInvalidated(page);
      await nav(page, 'Build').click();
      await expect(page.locator('.unsupported-block')).toHaveCount(0);
    });

    test('new Fork complete edits are rejected without Apply while a modeled retained plan applies exactly', async ({ page }) => {
      await syntheticCaster(page, false);
      await openOptimizer(page);
      await boundRealCandidatePool(page);
      const before = await groups(page);
      const beforeDps = numeric(await page.locator('.stat-row').filter({ hasText: 'Total DPS' }).locator('dd').innerText());
      await completeSearch(page);
      await expect(baseline(page)).toHaveCount(0);
      const rejected = page.locator('ol.support-diagnostic-excluded');
      await expect(rejected).toHaveCount(1);
      await rejected.locator('..').locator(':scope > summary').click();
      const forkEdits = rejected.locator(':scope > li > details')
        .filter({ has: page.locator(`.support-diagnostic-gems [data-skill-id="${fork}"]`) });
      expect(await forkEdits.count(), 'The actual Fork edit must be evaluated and receipted, not absent from the pool').toBeGreaterThan(0);
      const single = forkEdits.filter({ has: page.locator('.support-diagnostic-gems > li:only-child') });
      await expect(single).toHaveCount(1);
      for (let index = 0; index < await forkEdits.count(); index++) {
        const edit = forkEdits.nth(index);
        await edit.locator('summary').click();
        const reasons = edit.locator(':scope > ul:not(.support-diagnostic-gems) > li');
        await expect(reasons).toHaveCount(1);
        expectForkIdentity(await reasons.innerText());
        await expect(edit.getByRole('button', { name: 'Apply', exact: true })).toHaveCount(0);
        await expect(edit.locator('.support-plan')).toHaveCount(0);
      }
      await expect(page.locator(`.support-plan [data-skill-id="${fork}"]`)).toHaveCount(0);
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
        await nav(page, name).click();
        await expect(page.locator(name === 'Calcs' ? '.calcs-page' : name === 'Tree' ? '.tree-canvas' : '.upgrade-paths')).toBeVisible();
        await expect(run(page)).toHaveCount(0);
        await nav(page, 'Skills').click();
        await expect.poll(() => rejectedReceipt(rejected)).toEqual(excluded);
        await expect.poll(() => recommendationsReceipt(page)).toEqual(recommendations);
        await expect(first.getByRole('button', { name: 'Apply', exact: true })).toBeEnabled();
      }
      await first.getByRole('button', { name: 'Apply', exact: true }).click();
      await expect(page.locator('.topbar-busy')).toHaveCount(0);
      await expect.poll(async () => {
        const applied = (await groups(page))[0].gems;
        return [applied[0], ...applied.slice(1).map(gem => ({ ...gem, stat_set_index: gem.stat_set_index ?? 1 }))];
      }).toEqual([before[0].gems[0], ...expected]);
      expect((await groups(page)).slice(1)).toEqual(before.slice(1));
      await expect.poll(async () => Math.abs(numeric(await page.locator('.stat-row')
        .filter({ hasText: 'Total DPS' }).locator('dd').innerText()) - preview)).toBeLessThanOrEqual(0.06);
      await expectInvalidated(page);
      await nav(page, 'Build').click();
      await expect(page.locator('.unsupported-block')).toHaveCount(0);
    });
  });
}
