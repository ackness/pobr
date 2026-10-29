import { expect, test, type Locator, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { pageLink, statRow } from './shell';
import { closeInspector, detailValue, importBuild, openStat } from './skillsAndCalcs';

// A committed PoB2 build whose Armour bucket holds a conditional line that is not met
// ("if you haven't been Hit Recently") next to active INC and MORE lines.
const code = readFileSync(
  fileURLToPath(new URL('../../examples/demo-bd-test/builds/mercenary-tactician-wolf-pack/code.txt', import.meta.url)),
  'utf8',
).trim();

const num = (text: string | null) => Number((text ?? '').replace(/[^0-9.-]/g, ''));

/** Click a result in the left-hand panel and wait for the inspector to show its breakdown. */
async function inspectFromRail(page: Page, label: string | RegExp, heading: string): Promise<Locator> {
  await statRow(page, label).first().click();
  const inspector = page.locator('.insp');
  await expect(inspector.locator('h2')).toHaveText(heading);
  await expect(inspector.locator('div.breakdown-detail').first()).toBeVisible({ timeout: 90_000 });
  return inspector;
}

test('a stat breakdown counts only the lines the calculation applies', async ({ page }) => {
  await importBuild(page, code);
  const armour = await openStat(page, 'Armour');
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
  const shown = num(await detailValue(armour).textContent());
  expect(shown).toBeGreaterThan(0);
  // The header rounds values of 1000 and above to whole numbers.
  expect(Math.abs(num(await steps.locator('tr.formula-final .formula-result').textContent()) - shown)).toBeLessThanOrEqual(0.5);

  // Exactly the lines the calculation skips carry the "not active" tag; the rest are applied.
  const active = (await rows.count()) - (await inactive.count());
  expect(active).toBeGreaterThan(0);
  await expect(armour.locator('.mod-inactive-tag')).toHaveCount(await inactive.count());
});

test('headline sidebar stats open the lines behind them', async ({ page }) => {
  await importBuild(page, code);

  // Life is aggregated as MaximumLife and resistances partly under a shared name; each still
  // opens from its value in the results panel.
  for (const [label, heading] of [
    [/^Life$/, 'Life'],
    [/^Fire Res\.$/, 'Fire Res.'],
    [/^Chaos Res\.$/, 'Chaos Res.'],
  ] as const) {
    const inspector = await inspectFromRail(page, label, heading);
    await expect(inspector.locator('.breakdown-table tbody tr:not(.is-inactive)')).not.toHaveCount(0);
    if (heading === 'Fire Res.') {
      // Fire resistance: the steps go from the modifier total (86) to the capped value shown (75).
      const steps = inspector.locator('.breakdown-formula tr.formula-step');
      await expect(steps.first().locator('.formula-result')).toHaveText('86');
      await expect(inspector.locator('tr.formula-cap th')).toHaveText('Capped at the maximum');
      await expect(inspector.locator('tr.formula-cap .formula-operand')).toHaveText('≤ 75');
      const shown = num(await inspector.locator('.insp-value').textContent());
      expect(shown).toBe(75);
      expect(num(await inspector.locator('tr.formula-final .formula-result').textContent())).toBe(shown);
    }
    await closeInspector(page);
  }

  // Skill speed starts from the weapon or skill base rate, which is not a modifier line.
  const speed = await openStat(page, 'ActionRate');
  await expect(speed.locator('tr.formula-base th')).toContainText('Starting value');
  expect(num(await speed.locator('tr.formula-base .formula-result').textContent())).toBeGreaterThan(0);
  expect(num(await speed.locator('tr.formula-final .formula-result').textContent())).toBeCloseTo(
    num(await detailValue(speed).textContent()),
    2,
  );

  // Crit chance: only hits can crit, so the chance to hit is one of the steps.
  const crit = await openStat(page, 'CritChance');
  await expect(crit.locator('.breakdown-formula tr.formula-final')).toHaveCount(1);
  for (const id of ['EnergyShield', 'Evasion', 'Spirit', 'CritMultiplier']) {
    const detail = await openStat(page, id);
    await expect(detail.locator('.breakdown-formula tr.formula-final')).toHaveCount(1);
  }
});

test('hit chance shows the accuracy check against enemy evasion', async ({ page }) => {
  // An attack build whose accuracy does not quite guarantee a hit.
  const titan = readFileSync(
    fileURLToPath(new URL('../../examples/demo-bd-test/builds/warrior-titan-shield-wall/code.txt', import.meta.url)),
    'utf8',
  ).trim();
  await importBuild(page, titan);

  // Accuracy is a stat of its own, built from its modifier lines.
  const accuracy = await openStat(page, 'Accuracy');
  const rating = num(await detailValue(accuracy).textContent());
  expect(rating).toBeGreaterThan(0);
  await expect(accuracy.locator('.breakdown-table tbody tr:not(.is-inactive)')).not.toHaveCount(0);
  expect(Math.abs(num(await accuracy.locator('tr.formula-final .formula-result').textContent()) - rating)).toBeLessThanOrEqual(0.5);

  // Hit chance starts from that rating and ends at the value shown, below 100%.
  const hit = await openStat(page, 'HitChance');
  await expect(hit.locator('tr.formula-accuracy_rating th')).toContainText('Accuracy rating');
  expect(Math.abs(num(await hit.locator('tr.formula-accuracy_rating .formula-result').textContent()) - rating)).toBeLessThanOrEqual(0.5);
  await expect(hit.locator('tr.formula-vs_evasion th')).toContainText('Against enemy evasion');
  const shown = num(await detailValue(hit).textContent());
  expect(shown).toBeGreaterThan(90);
  expect(shown).toBeLessThan(100);
  expect(num(await hit.locator('tr.formula-final .formula-result').textContent())).toBeCloseTo(shown, 1);
});

test('average hit and total DPS show how they are built', async ({ page }) => {
  const titan = readFileSync(
    fileURLToPath(new URL('../../examples/demo-bd-test/builds/warrior-titan-shield-wall/code.txt', import.meta.url)),
    'utf8',
  ).trim();
  await importBuild(page, titan);

  // Average hit: one row per damage type; the averages add up to the non-critical hit the
  // steps start from, and the steps end at the value shown.
  const average = await openStat(page, 'TotalHitAvg');
  const table = average.locator('.damage-type-table');
  await expect(table.locator('tbody tr')).toHaveCount(5);
  await expect(table.locator('tbody tr.damage-physical th')).toHaveText('Physical');
  const total = num(await table.locator('tfoot td').first().textContent());
  expect(total).toBeGreaterThan(0);
  expect(num(await average.locator('tr.formula-non_crit_hit .formula-result').textContent())).toBeCloseTo(total, 1);
  await expect(average.locator('tr.formula-crit_hits th')).toContainText('Averaged with critical hits');
  const shownAverage = num(await detailValue(average).textContent());
  expect(shownAverage).toBeGreaterThan(total);
  expect(Math.abs(num(await average.locator('tr.formula-final .formula-result').textContent()) - shownAverage)).toBeLessThanOrEqual(0.5);

  // The increased column is backed by modifier lines: those shared by every type plus the
  // type's own add up to the physical row's increase.
  const incOf = async (group: string) => {
    const lines = average.locator(`.damage-lines-${group}`);
    if ((await lines.count()) === 0) return 0;
    await lines.locator('summary').click();
    // A group's header counts the lines it applies, and only those.
    const applied = lines.locator('tbody tr:not(.is-inactive)');
    expect(num(await lines.locator('summary .breakdown-count').textContent())).toBe(await applied.count());
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
  const dps = await openStat(page, 'TotalDPS');
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
  const shownDps = num(await detailValue(dps).textContent());
  expect(Math.abs(num(await dps.locator('tr.formula-final .formula-result').textContent()) - shownDps)).toBeLessThanOrEqual(0.5);
  // The enemy's own lines are listed per damage type below the table.
  await expect(dps.locator('.enemy-lines details')).not.toHaveCount(0);
});

const loadBuild = async (page: Page, build: string) => {
  const text = readFileSync(
    fileURLToPath(new URL(`../../examples/demo-bd-test/builds/${build}/code.txt`, import.meta.url)),
    'utf8',
  ).trim();
  await importBuild(page, text);
};
const stepResult = async (row: Locator) => num(await row.locator('.formula-result').textContent());

test('combined DPS adds the damage over time to the hits', async ({ page }) => {
  // A hit build with two ailments: the results panel's combined DPS opens steps that start at
  // the hit DPS and add the damage over time, which itself lists each ailment.
  await loadBuild(page, 'warrior-titan-shield-wall');
  await page.locator('#character-stats .rail-hero-value.is-link').click();
  const inspector = page.locator('.insp');
  await expect(inspector.locator('h2')).toHaveText('Combined DPS');
  await expect(inspector.locator('tr.formula-hit_dps')).toBeVisible({ timeout: 90_000 });
  const hit = await stepResult(inspector.locator('tr.formula-hit_dps'));
  const total = await stepResult(inspector.locator('tr.formula-dot_dps'));
  expect(hit).toBeGreaterThan(0);
  expect(total).toBeGreaterThan(hit);
  expect(Math.abs(total - num(await inspector.locator('.insp-value').textContent()))).toBeLessThanOrEqual(0.5);

  // The inspector hands the same stat over to the Calcs page.
  await inspector.getByRole('button', { name: 'Open in Calculations' }).click();
  await expect(page).toHaveURL(/\/calcs$/);
  await expect(page.locator('#calcs-detail-heading')).toHaveText('Combined DPS');
  await expect(page.locator('#breakdown-CombinedDPS')).toHaveAttribute('aria-pressed', 'true');
  const combined = page.locator('#calcs-detail');
  await expect(combined.locator('tr.formula-hit_dps')).toBeVisible({ timeout: 90_000 });
  expect(await stepResult(combined.locator('tr.formula-dot_dps'))).toBeCloseTo(total, 1);

  const dot = await openStat(page, 'TotalDotDPS');
  await expect(dot.locator('tr.formula-ignite_dps th')).toHaveText('Ignite DPS');
  await expect(dot.locator('tr.formula-bleed_dps')).toHaveCount(1);
  expect(await stepResult(dot.locator('tr.formula-final'))).toBeCloseTo(total - hit, 0);
});

test("a skill's own damage over time is listed per type", async ({ page }) => {
  // One row per type with its scaling and the enemy's multiplier, adding up to the first step.
  await loadBuild(page, 'sorceress-chronomancer-essence-drain');
  // The row is "DoT DPS"; the inspector names the stat it opens.
  const own = await inspectFromRail(page, /^DoT DPS$/, 'Total DoT DPS');
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
  await pageLink(page, 'Calcs').click();
  const ignite = await openStat(page, 'IgniteDPS');
  const source = await stepResult(ignite.locator('tr.formula-ailment_source'));
  expect(source).toBeGreaterThan(0);
  expect(await stepResult(ignite.locator('tr.formula-per_second'))).toBeLessThan(source);
  await expect(ignite.locator('tr.formula-active_stacks')).toHaveCount(1);
  expect(
    Math.abs(await stepResult(ignite.locator('tr.formula-final')) - num(await detailValue(ignite).textContent())),
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
  await pageLink(page, 'Calcs').click();
  const armour = await openStat(page, 'Armour');
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

  // From the results panel: a number with a calculation behind it opens that calculation.
  const life = await inspectFromRail(page, /^Life$/, 'Life');
  await expect(life.locator('.breakdown-formula')).toBeVisible();
  await closeInspector(page);

  await pageLink(page, 'Calcs').click();
  const lifeRow = page.locator('#breakdown-Life');
  await expect(lifeRow).toHaveAttribute('data-detail-kind', 'steps');
  await openStat(page, 'Life');
  await expect(page.locator('#calcs-detail .breakdown-formula')).toBeVisible();

  const cost = await openStat(page, 'ManaCost');
  await expect(page.locator('#breakdown-ManaCost')).toHaveAttribute('data-detail-kind', 'sources');
  await expect(cost.locator('.breakdown-sources')).toBeVisible();
  await expect(cost.locator('.breakdown-sources')).toContainText('Support cost multiplier');

  // A stat chosen from the results panel resets the list filters so the opened stat is not hidden.
  await page.locator('.calcs-search').fill('ManaCost');
  await statRow(page, /^Effective HP$/).first().click();
  await page.locator('.insp').getByRole('button', { name: 'Open in Calculations' }).click();
  await expect(page.locator('.calcs-search')).toHaveValue('');
  const ehp = page.locator('#calcs-detail');
  await expect(page.locator('#breakdown-TotalEHP')).toHaveAttribute('data-detail-kind', 'derived');
  await expect(ehp.locator('.derived-formula')).toBeVisible({ timeout: 90_000 });
  await expect(ehp.locator('.breakdown-derived')).toContainText('Enemy damage per hit');
  await expect(ehp.locator('.breakdown-derived')).toContainText('Resource pools used upstream');
  await ehp.scrollIntoViewIfNeeded();
  await ehp.screenshot({ path: 'test-results/calcs-derived.png' });

  const strength = await openStat(page, 'Str');
  await expect(page.locator('#breakdown-Str')).toHaveAttribute('data-detail-kind', 'sources');
  await expect(strength.locator('.breakdown-sources')).toContainText('Strength');
  await expect(strength.locator('.breakdown-sources tbody tr:not(.is-inactive)')).not.toHaveCount(0);
  await strength.screenshot({ path: 'test-results/calcs-strength-sources.png' });

  const burst = await openStat(page, 'AverageBurstDamage');
  await expect(page.locator('#breakdown-AverageBurstDamage')).toHaveAttribute('data-detail-kind', 'derived');
  await expect(burst.locator('.derived-formula')).toContainText('Hits in one burst');
  await expect(burst.locator('.breakdown-derived')).toContainText('Average Damage');
  expect(num(await detailValue(burst).textContent())).toBeGreaterThan(0);
  await burst.screenshot({ path: 'test-results/calcs-burst-derived.png' });

  const simulation = await openStat(page, 'NumberOfDamagingHits');
  await expect(page.locator('#breakdown-NumberOfDamagingHits')).toHaveAttribute('data-detail-kind', 'none');
  await expect(simulation.getByRole('status')).toHaveText('No breakdown available for this stat.');
  // A result without a calculation behind it is not offered as a click target in the results panel.
  for (const label of await page.locator('.calcs-metric-row[data-detail-kind="none"] > span').allTextContents()) {
    const row = statRow(page, new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}$`));
    if (await row.count()) await expect(row.first().locator('xpath=self::button')).toHaveCount(0);
  }

  // Reopening a detail after an edit must describe the new calculation.
  await page.locator('.shell-builds-button').click();
  await page.getByLabel('Level', { exact: true }).fill('75');
  await pageLink(page, 'Calcs').click();
  const reopened = await openStat(page, 'Life');
  await expect(reopened.locator('.breakdown-formula')).toBeVisible();
  await expect.poll(async () => Math.round(num(await reopened.locator('.formula-final .formula-result').textContent())))
    .toBe(Math.round(num(await detailValue(reopened).textContent())));
});
