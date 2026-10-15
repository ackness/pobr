import { expect, test, type Locator, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// A committed PoB2 build whose Armour bucket holds a conditional line that is not met
// ("if you haven't been Hit Recently") next to active INC and MORE lines.
const code = readFileSync(
  fileURLToPath(new URL('../../examples/demo-bd-test/builds/mercenary-tactician-wolf-pack/code.txt', import.meta.url)),
  'utf8',
).trim();

const num = (text: string | null) => Number((text ?? '').replace(/[^0-9.-]/g, ''));

test('a stat breakdown counts only the lines the calculation applies', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();
  await page.getByRole('navigation', { name: 'Main navigation' }).getByRole('button', { name: 'Calcs', exact: true }).click();

  const armour = page.locator('#breakdown-Armour');
  await armour.locator('.breakdown-toggle').click();
  const rows = armour.locator('.breakdown-table tbody tr');
  const inactive = armour.locator('tr.is-inactive');
  await expect(inactive).not.toHaveCount(0);
  await expect(inactive.first().locator('.mod-inactive-tag')).toHaveText('not active');
  await expect(armour.locator('.breakdown-note:not(.breakdown-totals-note)')).toBeVisible();

  // The steps count the equipped items' armour, the increases and the more multiplier, and
  // end at the value shown.
  const steps = armour.locator('.breakdown-formula');
  await expect(steps.locator('tr.formula-flat th')).toHaveText('Flat modifiers (BASE)');
  await expect(steps.locator('tr.formula-increased')).toHaveCount(1);
  await expect(steps.locator('tr.formula-more')).toHaveCount(1);
  const shown = num(await armour.locator('.breakdown-value').textContent());
  expect(shown).toBeGreaterThan(0);
  // The header rounds values of 1000 and above to whole numbers.
  expect(Math.abs(num(await steps.locator('tr.formula-final .formula-result').textContent()) - shown)).toBeLessThanOrEqual(0.5);

  const active = (await rows.count()) - (await inactive.count());
  await expect(armour.locator('.breakdown-count')).toHaveText(`${active} mods`);
});

test('headline sidebar stats open the lines behind them', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();

  // Life is aggregated as MaximumLife and resistances partly under a shared name; each still
  // expands from the sidebar value.
  const sidebar = page.locator('#character-stats');
  for (const [label, id] of [
    ['Life', 'Life'],
    ['Fire Res.', 'FireResist'],
    ['Chaos Res.', 'ChaosResist'],
  ]) {
    await sidebar.getByRole('button', { name: new RegExp(`^${label.replace('.', '\\.')}`) }).first().click();
    const item = page.locator(`#breakdown-${id}`);
    await expect(item).toHaveClass(/is-open/);
    await expect(item.locator('.breakdown-table tbody tr:not(.is-inactive)')).not.toHaveCount(0);
  }

  // Fire resistance: the steps go from the modifier total (86) to the capped value shown (75).
  await sidebar.getByRole('button', { name: /^Fire Res\.\s*\d/ }).click();
  const fire = page.locator('#breakdown-FireResist');
  const steps = fire.locator('.breakdown-formula tr.formula-step');
  await expect(steps.first().locator('.formula-result')).toHaveText('86');
  await expect(fire.locator('tr.formula-cap th')).toHaveText('Capped at the maximum');
  await expect(fire.locator('tr.formula-cap .formula-operand')).toHaveText('≤ 75');
  const shown = num(await fire.locator('.breakdown-value').textContent());
  expect(shown).toBe(75);
  expect(num(await fire.locator('tr.formula-final .formula-result').textContent())).toBe(shown);

  // Skill speed starts from the weapon or skill base rate, which is not a modifier line.
  await page.locator('#breakdown-ActionRate .breakdown-toggle').click();
  const speed = page.locator('#breakdown-ActionRate');
  await expect(speed.locator('tr.formula-base th')).toContainText('Starting value');
  expect(num(await speed.locator('tr.formula-base .formula-result').textContent())).toBeGreaterThan(0);
  expect(num(await speed.locator('tr.formula-final .formula-result').textContent())).toBeCloseTo(
    num(await speed.locator('.breakdown-value').textContent()),
    2,
  );

  // Crit chance: only hits can crit, so the chance to hit is one of the steps.
  await page.locator('#breakdown-CritChance .breakdown-toggle').click();
  const crit = page.locator('#breakdown-CritChance');
  await expect(crit.locator('.breakdown-formula tr.formula-final')).toHaveCount(1);
  for (const id of ['EnergyShield', 'Evasion', 'Spirit', 'CritMultiplier']) {
    await page.locator(`#breakdown-${id} .breakdown-toggle`).click();
    await expect(page.locator(`#breakdown-${id} .breakdown-formula tr.formula-final`)).toHaveCount(1);
  }
});

test('hit chance shows the accuracy check against enemy evasion', async ({ page }) => {
  // An attack build whose accuracy does not quite guarantee a hit.
  const titan = readFileSync(
    fileURLToPath(new URL('../../examples/demo-bd-test/builds/warrior-titan-shield-wall/code.txt', import.meta.url)),
    'utf8',
  ).trim();
  await page.goto('/');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(titan);
  await page.locator('.import-submit').click();
  await page.getByRole('navigation', { name: 'Main navigation' }).getByRole('button', { name: 'Calcs', exact: true }).click();

  // Accuracy is a stat of its own, built from its modifier lines.
  const accuracy = page.locator('#breakdown-Accuracy');
  await accuracy.locator('.breakdown-toggle').click();
  const rating = num(await accuracy.locator('.breakdown-value').textContent());
  expect(rating).toBeGreaterThan(0);
  await expect(accuracy.locator('.breakdown-table tbody tr:not(.is-inactive)')).not.toHaveCount(0);
  expect(Math.abs(num(await accuracy.locator('tr.formula-final .formula-result').textContent()) - rating)).toBeLessThanOrEqual(0.5);

  // Hit chance starts from that rating and ends at the value shown, below 100%.
  const hit = page.locator('#breakdown-HitChance');
  await hit.locator('.breakdown-toggle').click();
  await expect(hit.locator('tr.formula-accuracy_rating th')).toContainText('Accuracy rating');
  expect(Math.abs(num(await hit.locator('tr.formula-accuracy_rating .formula-result').textContent()) - rating)).toBeLessThanOrEqual(0.5);
  await expect(hit.locator('tr.formula-vs_evasion th')).toContainText('Against enemy evasion');
  const shown = num(await hit.locator('.breakdown-value').textContent());
  expect(shown).toBeGreaterThan(90);
  expect(shown).toBeLessThan(100);
  expect(num(await hit.locator('tr.formula-final .formula-result').textContent())).toBeCloseTo(shown, 1);
});

test('average hit and total DPS show how they are built', async ({ page }) => {
  const titan = readFileSync(
    fileURLToPath(new URL('../../examples/demo-bd-test/builds/warrior-titan-shield-wall/code.txt', import.meta.url)),
    'utf8',
  ).trim();
  await page.goto('/');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(titan);
  await page.locator('.import-submit').click();
  await page.getByRole('navigation', { name: 'Main navigation' }).getByRole('button', { name: 'Calcs', exact: true }).click();

  // Average hit: one row per damage type; the averages add up to the non-critical hit the
  // steps start from, and the steps end at the value shown.
  const average = page.locator('#breakdown-TotalHitAvg');
  await average.locator('.breakdown-toggle').click();
  const table = average.locator('.damage-type-table');
  await expect(table.locator('tbody tr')).toHaveCount(5);
  await expect(table.locator('tbody tr.damage-physical th')).toHaveText('Physical');
  const total = num(await table.locator('tfoot td').first().textContent());
  expect(total).toBeGreaterThan(0);
  expect(num(await average.locator('tr.formula-non_crit_hit .formula-result').textContent())).toBeCloseTo(total, 1);
  await expect(average.locator('tr.formula-crit_hits th')).toContainText('Averaged with critical hits');
  const shownAverage = num(await average.locator('.breakdown-value').textContent());
  expect(shownAverage).toBeGreaterThan(total);
  expect(Math.abs(num(await average.locator('tr.formula-final .formula-result').textContent()) - shownAverage)).toBeLessThanOrEqual(0.5);

  // The increased column is backed by modifier lines: those shared by every type plus the
  // type's own add up to the physical row's increase.
  const incOf = async (group: string) => {
    const lines = average.locator(`.damage-lines-${group}`);
    if ((await lines.count()) === 0) return 0;
    await lines.locator('summary').click();
    const values = await lines
      .locator('tbody tr:not(.is-inactive)', { has: page.locator('.mod-type-inc') })
      .locator('.mod-value')
      .allTextContents();
    return values.reduce((sum, text) => sum + num(text), 0);
  };
  const listed = (await incOf('shared')) + (await incOf('physical'));
  expect(listed).toBeGreaterThan(0);
  expect(listed).toBeCloseTo(num(await table.locator('tbody tr.damage-physical td').nth(1).textContent()), 1);

  // The build gains extra damage of other types, so the ranges before that are listed with
  // the lines that move them.
  const sources = average.locator('.damage-source-table');
  await expect(sources.locator('tbody tr.damage-physical')).toHaveCount(1);
  await expect(average.locator('.damage-lines-physical-gain tbody tr:not(.is-inactive)')).not.toHaveCount(0);

  // Total DPS: starts from that average hit and multiplies in rate and chance to hit.
  const dps = page.locator('#breakdown-TotalDPS');
  await dps.locator('.breakdown-toggle').click();
  expect(Math.abs(num(await dps.locator('tr.formula-average_hit .formula-result').textContent()) - shownAverage)).toBeLessThanOrEqual(0.5);
  await expect(dps.locator('tr.formula-uses_per_second')).toHaveCount(1);
  await expect(dps.locator('tr.formula-chance_to_hit th')).toHaveText('Chance to hit');
  // The enemy step is split by damage type: the rows start at the average hit and end at
  // the step's result.
  const enemy = dps.locator('.mitigation-table');
  await expect(enemy.locator('tbody tr.damage-physical td').nth(1)).toHaveText('armour');
  const totals = enemy.locator('tfoot td.mod-value');
  expect(Math.abs(num(await totals.first().textContent()) - shownAverage)).toBeLessThanOrEqual(0.5);
  expect(num(await totals.last().textContent())).toBeCloseTo(
    num(await dps.locator('tr.formula-enemy_mitigation .formula-result').textContent()),
    1,
  );
  const shownDps = num(await dps.locator('.breakdown-value').textContent());
  expect(Math.abs(num(await dps.locator('tr.formula-final .formula-result').textContent()) - shownDps)).toBeLessThanOrEqual(0.5);
  // The enemy's own lines are listed per damage type below the table.
  await expect(dps.locator('.enemy-lines details')).not.toHaveCount(0);
});

const loadBuild = async (page: Page, build: string) => {
  const text = readFileSync(
    fileURLToPath(new URL(`../../examples/demo-bd-test/builds/${build}/code.txt`, import.meta.url)),
    'utf8',
  ).trim();
  await page.goto('/');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(text);
  await page.locator('.import-submit').click();
};
const stepResult = async (row: Locator) => num(await row.locator('.formula-result').textContent());

test('combined DPS adds the damage over time to the hits', async ({ page }) => {
  // A hit build with two ailments: the sidebar's combined DPS opens steps that start at the
  // hit DPS and add the damage over time, which itself lists each ailment.
  await loadBuild(page, 'warrior-titan-shield-wall');
  await page.locator('.main-skill-section').getByRole('button', { name: /^Combined DPS/ }).click();
  const combined = page.locator('#breakdown-CombinedDPS');
  await expect(combined).toHaveClass(/is-open/);
  const hit = await stepResult(combined.locator('tr.formula-hit_dps'));
  const total = await stepResult(combined.locator('tr.formula-dot_dps'));
  expect(hit).toBeGreaterThan(0);
  expect(total).toBeGreaterThan(hit);
  expect(Math.abs(total - num(await combined.locator('.breakdown-value').textContent()))).toBeLessThanOrEqual(0.5);

  const dot = page.locator('#breakdown-TotalDotDPS');
  await dot.locator('.breakdown-toggle').click();
  await expect(dot.locator('tr.formula-ignite_dps th')).toHaveText('Ignite DPS');
  await expect(dot.locator('tr.formula-bleed_dps')).toHaveCount(1);
  expect(await stepResult(dot.locator('tr.formula-final'))).toBeCloseTo(total - hit, 0);
});

test("a skill's own damage over time is listed per type", async ({ page }) => {
  // One row per type with its scaling and the enemy's multiplier, adding up to the first step.
  await loadBuild(page, 'sorceress-chronomancer-essence-drain');
  await page.locator('.main-skill-section').getByRole('button', { name: /^DoT DPS/ }).click();
  const own = page.locator('#breakdown-TotalDotDPS');
  await expect(own).toHaveClass(/is-open/);
  const table = own.locator('.skill-dot-table');
  await expect(table.locator('tbody tr')).toHaveCount(1);
  await expect(table.locator('tbody tr.damage-chaos th')).toHaveText('Chaos');
  expect(num(await table.locator('tbody tr td').last().textContent())).toBeCloseTo(
    await stepResult(own.locator('tr.formula-skill_dot')),
    1,
  );
  await own.locator('.damage-lines-chaos summary').click();
  await expect(own.locator('.damage-lines-chaos tbody tr')).not.toHaveCount(0);
});

test('an ailment shows the hit it comes from and how many are active', async ({ page }) => {
  // Ignite on a hit build: the steps run from the hit to the shown DPS, and the table below
  // gives the numbers behind the hit and the active instance count.
  await loadBuild(page, 'warrior-titan-shield-wall');
  await page.getByRole('navigation', { name: 'Main navigation' }).getByRole('button', { name: 'Calcs', exact: true }).click();
  const ignite = page.locator('#breakdown-IgniteDPS');
  await ignite.locator('.breakdown-toggle').click();
  const source = await stepResult(ignite.locator('tr.formula-ailment_source'));
  expect(source).toBeGreaterThan(0);
  expect(await stepResult(ignite.locator('tr.formula-per_second'))).toBeLessThan(source);
  await expect(ignite.locator('tr.formula-active_stacks')).toHaveCount(1);
  expect(
    Math.abs(await stepResult(ignite.locator('tr.formula-final')) - num(await ignite.locator('.breakdown-value').textContent())),
  ).toBeLessThanOrEqual(0.5);
  const detail = ignite.locator('.ailment-table');
  await expect(detail.locator('tr.ailment-applyChance th')).toHaveText('Chance for a hit to apply it');
  expect(num(await detail.locator('tr.ailment-sourceHit td').textContent())).toBeGreaterThan(0);
  expect(num(await detail.locator('tr.ailment-maxStacks td').textContent())).toBeGreaterThanOrEqual(1);
  // The lines behind the inputs of the steps are listed below.
  await expect(ignite.locator('.ailment-lines details')).not.toHaveCount(0);
});

test('a defence lists what each item slot adds on its own', async ({ page }) => {
  // Lines scoped to one item slot: the per-slot rows add up to the step of the formula.
  await loadBuild(page, 'warrior-titan-shield-wall');
  await page.getByRole('navigation', { name: 'Main navigation' }).getByRole('button', { name: 'Calcs', exact: true }).click();
  const armour = page.locator('#breakdown-Armour');
  await armour.locator('.breakdown-toggle').click();
  const slots = armour.locator('.defence-slots-table');
  await expect(slots.locator('tbody tr')).not.toHaveCount(0);
  expect(num(await slots.locator('tfoot td').textContent())).toBeCloseTo(
    num(await armour.locator('tr.formula-slot_scaled .formula-operand').textContent()),
    1,
  );
});

test('on-demand details cover steps, sources, derived inputs and an explicit empty state', async ({ page }) => {
  test.setTimeout(60_000);
  await loadBuild(page, 'monk-invoker-frost-bomb');
  const sidebar = page.locator('#character-stats');
  await sidebar.getByRole('button', { name: /^Life\s/ }).first().click();
  const life = page.locator('#breakdown-Life');
  await expect(life).toHaveAttribute('data-detail-kind', 'steps');
  await expect(life.locator('.breakdown-formula')).toBeVisible();

  await sidebar.getByRole('button', { name: /^Mana Cost\s/ }).first().click();
  const cost = page.locator('#breakdown-ManaCost');
  await expect(cost).toHaveAttribute('data-detail-kind', 'sources');
  await expect(cost.locator('.breakdown-sources')).toBeVisible();
  await expect(cost.locator('.breakdown-sources')).toContainText('Support cost multiplier');

  await page.locator('.calcs-search').fill('ManaCost');
  await sidebar.getByRole('button', { name: /^Effective HP\s/ }).first().click();
  await expect(page.locator('.calcs-search')).toHaveValue('');
  const ehp = page.locator('#breakdown-TotalEHP');
  await expect(ehp).toHaveAttribute('data-detail-kind', 'derived');
  await expect(ehp.locator('.derived-formula')).toBeVisible();
  await expect(ehp.locator('.breakdown-derived')).toContainText('Enemy damage per hit');
  await expect(ehp.locator('.breakdown-derived')).toContainText('Resource pools used upstream');
  await ehp.scrollIntoViewIfNeeded();
  await ehp.screenshot({ path: 'test-results/calcs-derived.png' });

  const strength = page.locator('#breakdown-Str');
  await strength.locator('.breakdown-toggle').click();
  await expect(strength).toHaveAttribute('data-detail-kind', 'none');
  await expect(strength.getByRole('status')).toHaveText('No breakdown available for this stat.');
  await expect(sidebar.locator('[data-detail-kind="none"]').first()).toHaveAttribute('title', 'No breakdown available for this stat.');

  // Reopening a detail after an edit must describe the new calculation.
  await page.getByRole('navigation', { name: 'Main navigation' }).getByRole('button', { name: 'Build', exact: true }).click();
  await page.getByLabel('Level', { exact: true }).fill('75');
  await sidebar.getByRole('button', { name: /^Life\s/ }).first().click();
  await expect(life.locator('.breakdown-formula')).toBeVisible();
  await expect.poll(async () => Math.round(num(await life.locator('.formula-final .formula-result').textContent())))
    .toBe(Math.round(num(await life.locator('.breakdown-value').textContent())));
});
