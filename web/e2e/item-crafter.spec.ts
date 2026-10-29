import { expect, test } from '@playwright/test';
import { openItemsAfterImport } from './shell';
import { lifeValue, settledText } from './upgradesPage';

test('craft a rare from a game base, save it and equip the previewed delta', async ({ page }, testInfo) => {
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: 85, class_name: 'Sorceress' },
    equipments: [{ inventoryId: 'Ring', x: 0, baseType: 'Sapphire Ring', name: 'Current Ring', frameType: 2,
      explicitMods: ['+20 to maximum Life'] }],
    talent_tree: { hashes: [], quest_stats: [] }, jewel_data: '[]',
    skills: [{ baseType: 'Fireball', support: false, properties: [{ type: 5, values: [['16', 0]] }] }],
  } }));
  await page.goto('/build');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticCrafterFixture');
  await page.locator('.import-submit').click();
  await openItemsAfterImport(page);
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 30_000 });
  const life = lifeValue(page);
  const before = await settledText(life);
  await page.getByRole('button', { name: 'Craft item', exact: true }).click();
  const crafter = page.getByRole('region', { name: 'Craft item', exact: true });
  await crafter.getByRole('combobox', { name: 'Item base', exact: true }).click();
  await page.getByRole('option', { name: /^Sapphire Ring/ }).click();
  await expect(crafter.locator('.item-crafter-base')).toContainText('Required level 12');
  await expect(crafter.locator('.item-crafter-base')).toContainText('+30% to Cold Resistance');
  await crafter.getByRole('spinbutton', { name: 'Item level', exact: true }).fill('50');
  await crafter.screenshot({ path: testInfo.outputPath('base-picker.png') });
  await crafter.getByRole('button', { name: 'Create draft', exact: true }).click();
  const editor = crafter.locator('.replacement-affix-editor');
  await editor.locator('summary').click();
  await editor.getByRole('searchbox', { name: 'Search affixes' }).fill('to maximum Life');
  await editor.getByRole('combobox', { name: 'Choose an affix and tier', exact: true }).click();
  await expect(page.getByRole('option', { name: /\+119 to maximum Life/ })).toHaveCount(0);
  await page.getByRole('option', { name: /\+99 to maximum Life/ }).click();
  await editor.locator('.affix-editor-add > .trade-primary').click();
  await expect(life).toHaveText(before);
  await editor.getByRole('button', { name: 'Maximum', exact: true }).click();
  await editor.getByRole('button', { name: 'Recalculate edited item', exact: true }).click();
  await expect(crafter.getByRole('textbox', { name: 'Complete item text' })).toHaveValue(/\+99 to maximum Life/);
  await crafter.locator('.replacement-position').filter({ hasText: /^Ring 1/ }).click();
  await expect(crafter.getByRole('button', { name: 'Apply to build · Ring 1', exact: true })).toBeVisible();
  const expectedLife = await crafter.locator('.upgrade-replacement-result tbody tr').filter({ hasText: /^Life(?=[0-9\s])/ }).locator('td').nth(1).innerText();
  await expect(life).toHaveText(before);
  await crafter.getByRole('button', { name: 'Save to library', exact: true }).click();
  await expect(crafter.getByRole('button', { name: 'Saved to library', exact: true })).toBeVisible();
  await expect(life).toHaveText(before);
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-library')!).items.at(-1));
  expect(saved).toMatchObject({ kind: 'item', slot: 'ring1' });
  expect(saved.text).toContain('+99 to maximum Life');
  expect(saved.text).toContain('Sapphire Ring');
  await page.screenshot({ path: testInfo.outputPath('crafted-rare-preview.png'), fullPage: true });
  await crafter.getByRole('button', { name: 'Apply to build · Ring 1', exact: true }).click();
  await expect(life).not.toHaveText(before);
  await expect.poll(async () => Number((await life.innerText()).replaceAll(',', ''))).toBeCloseTo(Number(expectedLife.replaceAll(',', '')), 0);
  await page.setViewportSize({ width: 320, height: 900 });
  await page.addStyleTag({ content: 'html { scrollbar-gutter: stable; }' });
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});
