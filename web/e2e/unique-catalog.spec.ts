import { expect, test } from '@playwright/test';
import { openItemsAfterImport } from './shell';
import { lifeValue, settledText } from './upgradesPage';
import { deflateSync } from 'node:zlib';
import { installItemClassFixture } from './itemClassFixture';

test('find a unique by modifier, choose a variant and rolls, then save and equip its preview', async ({ page }, testInfo) => {
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: 85, class_name: 'Sorceress' },
    equipments: [{ inventoryId: 'Ring', x: 0, baseType: 'Sapphire Ring', name: 'Current Ring', frameType: 2,
      explicitMods: ['+20 to maximum Life'] }],
    talent_tree: { hashes: [], quest_stats: [] }, jewel_data: '[]',
    skills: [{ baseType: 'Fireball', support: false, properties: [{ type: 5, values: [['16', 0]] }] }],
  } }));
  await page.goto('/build');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticUniqueFixture');
  await page.locator('.import-submit').click();
  await openItemsAfterImport(page);
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 30_000 });
  const life = lifeValue(page);
  const before = await settledText(life);
  await page.getByRole('button', { name: 'Unique catalogue', exact: true }).click();
  const catalogue = page.getByRole('region', { name: 'Unique catalogue', exact: true });
  await catalogue.getByRole('searchbox', { name: 'Search unique names and modifiers' }).fill('all attributes');
  await catalogue.getByRole('combobox', { name: 'Item class', exact: true }).click();
  await page.getByRole('option', { name: 'Amulet', exact: true }).click();
  await catalogue.getByRole('combobox', { name: 'Equipment slot', exact: true }).click();
  await page.getByRole('option', { name: 'Amulet', exact: true }).click();
  await catalogue.getByRole('button', { name: /^Astramentis/ }).click();
  await expect(catalogue).toContainText('Rolls default to the midpoint');
  await catalogue.getByRole('combobox', { name: 'Variant', exact: true }).click();
  await page.getByRole('option', { name: 'Pre 0.2.0', exact: true }).click();
  await catalogue.getByRole('button', { name: 'All maximum rolls', exact: true }).click();
  await page.setViewportSize({ width: 1280, height: 1100 });
  await catalogue.screenshot({ path: testInfo.outputPath('unique-selection.png') });
  await catalogue.getByRole('button', { name: 'Preview unique', exact: true }).click();
  const text = catalogue.getByRole('textbox', { name: 'Complete item text' });
  await expect(text).toHaveValue(/Selected Variant: 1/);
  await expect(text).toHaveValue(/\+100 to all Attributes/);
  await expect(catalogue.getByRole('button', { name: 'Apply to build · Amulet', exact: true })).toBeVisible();
  await expect(life).toHaveText(before);
  const expectedLife = await catalogue.locator('.swap-gain-card [data-stat="Life"] .swap-after').innerText();
  await catalogue.getByRole('button', { name: 'Save to item library', exact: true }).click();
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-library')!).items.at(-1));
  expect(saved).toMatchObject({ kind: 'item', slot: 'amulet' });
  expect(saved.text).toContain('Astramentis');
  expect(saved.text).toContain('+100 to all Attributes');
  await expect(life).toHaveText(before);
  await page.screenshot({ path: testInfo.outputPath('unique-preview.png'), fullPage: true });
  await catalogue.getByRole('button', { name: 'Apply to build · Amulet', exact: true }).click();
  await expect(life).not.toHaveText(before);
  await expect.poll(async () => Number((await life.innerText()).replaceAll(',', ''))).toBeCloseTo(Number(expectedLife.replaceAll(',', '')), 0);
  await page.setViewportSize({ width: 320, height: 900 });
  await page.addStyleTag({ content: 'html { scrollbar-gutter: stable; }' });
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});

for (const member of [false, true]) {
  test(`legacy Unique class labels retain fixed Body Armour and translated Ring with member=${member}`, async ({ page }) => {
    if (member) await installItemClassFixture(page, {
      scopes: { 'ItemClasses.Name': { Ring: 'Rings', 'Body Armour': 'Body Armours' } },
    }, {});
    const code = deflateSync('<PathOfBuilding2><Build level="85" className="Sorceress"/><Items/></PathOfBuilding2>').toString('base64url');
    await page.goto('/build');
    await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
    await page.locator('.import-submit').click();
    await openItemsAfterImport(page);
    await page.getByRole('button', { name: 'Unique catalogue', exact: true }).click();
    const classes = page.locator('.unique-catalogue .item-crafter-fields [role="combobox"]').first();
    await classes.click();
    await page.getByRole('option', { name: 'Body Armour', exact: true }).click();
    await page.locator('.shell-settings-button').click();
    await page.locator('.shell-settings').getByRole('combobox').first().click();
    await page.getByRole('option', { name: '简体中文', exact: true }).click();
    await page.locator('.shell-settings').getByRole('button', { name: '关闭', exact: true }).click();
    await expect(classes).toHaveText('胸甲');
    await classes.click();
    // The option itself carries the unchanged legacy class value.
    const ringOption = page.locator('[role="option"][data-value="Ring"]');
    await expect(ringOption).toHaveText('戒指');
    await ringOption.click();
    await expect(classes).toHaveText('戒指');
    await expect(page.locator('.unique-results .unique-result').first()).toBeVisible();
  });
}
