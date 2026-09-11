import { expect, test, type Page } from '@playwright/test';

async function caster(page: Page) {
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: 85, class_name: 'Sorceress' },
    equipments: [{ inventoryId: 'Ring', x: 0, baseType: 'Sapphire Ring', name: 'Current Ring', frameType: 2,
      explicitMods: ['40% increased Fire Damage', '+80 to maximum Life'] }],
    talent_tree: { hashes: [54447, 4739, 22419, 18407], quest_stats: [] }, jewel_data: '[]',
    skills: [{ baseType: 'Fireball', support: false, properties: [{ type: 5, values: [['16', 0]] }] }],
  } }));
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticGuidanceFixture');
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 30_000 });
}
const nav = (page: Page, name: string) => page.getByRole('navigation', { name: 'Main navigation' }).getByRole('button', { name, exact: true });
const dps = (page: Page) => page.locator('.stat-row').filter({ hasText: 'Total DPS' }).locator('dd');

test('current market Sum is a reference; complete replacement detects a downgrade before applying', async ({ page }) => {
  await caster(page);
  const before = await dps(page).innerText();
  await nav(page, 'Upgrades').click();
  await page.locator('.trade-position').filter({ hasText: /^Ring 1/ }).click();
  await page.getByRole('button', { name: 'Calculate affix scores', exact: true }).click();
  const reference = page.locator('.upgrade-score-reference');
  await expect(reference).toBeVisible();
  await expect(reference).toContainText('Current item · market Sum');
  const sum = Number((await reference.locator('div > strong').first().innerText()).replaceAll(',', ''));
  const href = await page.locator('.trade-market-link').first().getAttribute('href');
  const query = JSON.parse(new URL(href!).searchParams.get('q')!);
  expect(sum).toBeGreaterThan(0);
  expect(query.query.stats[0].value.min).toBeCloseTo(sum, 3);
  await page.getByRole('textbox', { name: 'Complete item text' }).fill('Rarity: NORMAL\nSapphire Ring');
  await page.getByRole('button', { name: 'Calculate replacement' }).click();
  await expect(page.locator('.upgrade-replacement-result')).toContainText('DPS decreases after this replacement');
  await expect(dps(page)).toHaveText(before);
  const preview = await page.locator('.upgrade-replacement-result tbody tr').first().locator('td').nth(1).innerText();
  await page.getByRole('button', { name: 'Apply to build · Ring 1', exact: true }).click();
  await expect(dps(page)).not.toHaveText(before);
  expect(Number((await dps(page).innerText()).replaceAll(',', ''))).toBeCloseTo(Number(preview.replaceAll(',', '')), 0);
  await expect(reference).toHaveCount(0);
  await page.getByRole('textbox', { name: 'Complete item text' }).fill('Rarity: NORMAL\nTwin Bow');
  await page.getByRole('button', { name: 'Calculate replacement' }).click();
  await expect(page.getByRole('alert')).toContainText('No compatible equipped position');
  await expect(page.getByRole('button', { name: /Apply to build/ })).toHaveCount(0);
});

test('shared goals link automatic supports and connected passive plans with real apply', async ({ page }) => {
  await caster(page);
  await nav(page, 'Upgrades').click();
  await page.getByRole('checkbox', { name: 'Prioritize capped elemental resistances' }).check();
  await page.locator('.upgrade-paths > button').nth(1).click();
  await expect(page.locator('.gem-optimizer-body')).toBeVisible();
  await expect(page.getByRole('checkbox', { name: 'Prioritize capped elemental resistances' })).toBeChecked();
  await expect.poll(async () => Number(await page.locator('.support-pool-summary strong').innerText())).toBeGreaterThan(20);
  await page.getByRole('button', { name: 'Calculate support combinations', exact: true }).click();
  const support = page.locator('.support-plan').first();
  await expect(support).toBeVisible({ timeout: 30_000 });
  await expect(support.locator('.support-plan-metrics')).toContainText('EHP');
  const before = await dps(page).innerText();
  await support.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(dps(page)).not.toHaveText(before);
  await nav(page, 'Upgrades').click();
  await page.locator('.trade-position').filter({ hasText: 'Skill and support gems' }).click();
  await page.getByRole('button', { name: 'Calculate affix scores', exact: true }).click();
  await expect(page.locator('.trade-gems')).toBeVisible();
  for (const card of await page.locator('.trade-gem-card').filter({ hasText: 'Skill adjustment' }).all()) {
    await expect(card.getByRole('link')).toHaveCount(0);
  }
  await page.locator('.upgrade-paths > button').nth(2).click();
  await expect(page.locator('.tree-planner-body')).toBeVisible();
  await expect(page.getByRole('checkbox', { name: 'Prioritize capped elemental resistances' })).toBeChecked();
  await page.locator('.tree-planner-run').click();
  const route = page.locator('.tree-planner-results > li').first();
  await expect(route).toBeVisible({ timeout: 30_000 });
  await route.getByRole('button', { name: 'Show on tree' }).click();
  await expect(page.locator('.node-planned-add').first()).toBeVisible();
  const treeBefore = await dps(page).innerText();
  await route.getByRole('button', { name: 'Apply plan' }).click();
  await expect(page.locator('.tree-planner-results > li')).toHaveCount(0);
  await expect(dps(page)).not.toHaveText(treeBefore);
  await expect(page.locator('.calc-error')).toHaveCount(0);
});
