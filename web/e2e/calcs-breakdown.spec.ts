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
  await expect(armour.locator('.breakdown-note')).toBeVisible();

  const summary = armour.locator('.breakdown-summary strong');
  const incTotal = num(await summary.nth(1).textContent());
  let activeInc = 0;
  for (const row of await rows.all()) {
    if ((await row.locator('.mod-type').textContent()) !== 'INC') continue;
    if (await row.evaluate((el) => el.classList.contains('is-inactive'))) continue;
    activeInc += num(await row.locator('.mod-value').textContent());
  }
  expect(activeInc).toBeCloseTo(incTotal, 6);
  expect(num(await summary.nth(2).textContent())).toBeGreaterThan(1);

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

  // Fire resistance: 86% of modifiers, shown capped at 75%.
  await sidebar.getByRole('button', { name: /^Fire Res\./ }).click();
  const fire = page.locator('#breakdown-FireResist');
  expect(num(await fire.locator('.breakdown-summary strong').first().textContent())).toBe(86);
  expect(num(await fire.locator('.breakdown-value').textContent())).toBe(75);
});
