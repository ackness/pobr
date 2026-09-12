import { expect, test } from '@playwright/test';

test('manual affix tiers respect item level and recalculate a whole item before application', async ({ page }) => {
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: 85, class_name: 'Sorceress' },
    equipments: [{ inventoryId: 'Ring', x: 0, baseType: 'Sapphire Ring', name: 'Current Ring', frameType: 2,
      explicitMods: ['40% increased Fire Damage', '+80 to maximum Life'] },
      { inventoryId: 'Ring', x: 1, baseType: 'Sapphire Ring', name: 'Second Ring', frameType: 2,
        explicitMods: ['+60 to maximum Life'] }],
    talent_tree: { hashes: [], quest_stats: [] }, jewel_data: '[]',
    skills: [{ baseType: 'Fireball', support: false, properties: [{ type: 5, values: [['16', 0]] }] }],
  } }));
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticAffixFixture');
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 30_000 });
  const life = page.locator('.stat-row').filter({ hasText: /^Life(?=[0-9\s])/ }).locator('dd');
  const before = await life.innerText();
  await page.getByRole('navigation', { name: 'Main navigation' }).getByRole('button', { name: 'Upgrades', exact: true }).click();
  const input = page.getByRole('textbox', { name: 'Complete item text' });
  await input.fill('Rarity: RARE\nCrafting Candidate\nSapphire Ring\nItem Level: 50\nImplicits: 1\n+20% to Cold Resistance\n+80 to maximum Life');
  await page.getByRole('button', { name: 'Calculate replacement', exact: true }).click();
  // Ring 2 ranks higher; an explicit Ring 1 choice must survive editing and recalculation.
  await page.locator('.replacement-position').filter({ hasText: /^Ring 1/ }).click();
  const editor = page.locator('.replacement-affix-editor');
  await editor.locator('summary').click();
  await expect(editor.locator('[data-affix-id="equipment:IncreasedLife6"]')).toBeVisible();
  await expect(editor.locator('.affix-editor-meta')).toContainText('Prefixes 1 / 3');
  await editor.locator('.affix-editor-row').getByRole('button', { name: 'Replace affix', exact: true }).click();
  await editor.getByRole('searchbox', { name: 'Search affixes' }).fill('to maximum Life');
  await editor.getByRole('button', { name: 'Choose an affix and tier', exact: true }).click();
  await expect(editor.getByRole('option', { name: /\+119 to maximum Life/ })).toHaveCount(0);
  await editor.getByRole('option', { name: /\+99 to maximum Life/ }).click();
  await expect(editor.locator('.affix-editor-preview')).toContainText('+92 to maximum Life');
  await editor.locator('.affix-editor-add > .trade-primary').click();
  await expect(page.getByRole('button', { name: 'Apply to build · Ring 1', exact: true })).toBeDisabled();
  await expect(life).toHaveText(before);
  await editor.getByRole('button', { name: 'Recalculate edited item', exact: true }).click();
  await expect(input).toHaveValue(/\+92 to maximum Life/);
  await expect(page.getByRole('button', { name: 'Apply to build · Ring 1', exact: true })).toBeEnabled();
  await expect(life).toHaveText(before);
  const expectedLife = await page.locator('.upgrade-replacement-result tbody tr').filter({ hasText: /^Life(?=[0-9\s])/ }).locator('td').nth(1).innerText();
  await editor.locator('summary').click();
  await page.setViewportSize({ width: 320, height: 900 });
  await page.addStyleTag({ content: 'html { scrollbar-gutter: stable; }' });
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.getByRole('button', { name: 'Apply to build · Ring 1', exact: true }).click();
  await expect(life).not.toHaveText(before);
  expect(Number((await life.innerText()).replaceAll(',', ''))).toBeCloseTo(Number(expectedLife.replaceAll(',', '')), 0);
});
