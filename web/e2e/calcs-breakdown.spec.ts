import { expect, test } from '@playwright/test';
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
  await sidebar.getByRole('button', { name: /^Fire Res\./ }).click();
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
