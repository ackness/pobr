import { expect, test, type Page } from '@playwright/test';
import { deflateSync } from 'node:zlib';
import { openItemsAfterImport } from './shell';
import { installItemClassFixture } from './itemClassFixture';
import { lifeValue, settledText } from './upgradesPage';
import { SMOKE } from './tags';

test('craft a rare from a game base, save it and equip the previewed delta', SMOKE, async ({ page }, testInfo) => {
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
  await expect(editor).toHaveAttribute('open', '');
  await editor.getByRole('searchbox', { name: 'Search modifiers' }).fill('to maximum Life');
  await editor.getByRole('combobox', { name: 'Choose a modifier and tier', exact: true }).click();
  await expect(page.getByRole('option', { name: /\+119 to maximum Life/ })).toHaveCount(0);
  await page.getByRole('option', { name: /\+99 to maximum Life/ }).click();
  await editor.locator('.affix-editor-add > .trade-primary').click();
  await expect(life).toHaveText(before);
  await editor.getByRole('button', { name: 'Maximum', exact: true }).click();
  await editor.getByRole('button', { name: 'Compare with current equipment', exact: true }).click();
  await expect(crafter.getByRole('textbox', { name: 'Complete item text' })).toHaveValue(/\+99 to maximum Life/);
  await crafter.locator('.replacement-position').filter({ hasText: /^Ring 1/ }).click();
  await expect(crafter.getByRole('button', { name: 'Apply to build · Ring 1', exact: true })).toBeVisible();
  const expectedLife = await crafter.locator('.swap-gain-card [data-stat="Life"] .swap-after').innerText();
  await expect(life).toHaveText(before);
  await crafter.getByRole('button', { name: 'Save to item library', exact: true }).click();
  await expect(crafter.getByRole('button', { name: 'Saved to item library', exact: true })).toBeVisible();
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

const emptyBuild = deflateSync('<PathOfBuilding2><Build level="85" className="Sorceress"/><Items/></PathOfBuilding2>').toString('base64url');
const classEnglish = { Ring: 'Rings', Staff: 'Staves', Warstaff: 'Quarterstaves', UtilityFlask: 'Charms', 'Body Armour': 'Body Armours' };
const classRecords = Object.fromEntries(Object.entries(classEnglish).map(([id, name], index) => [id, [name, `測試類別${index}`]])) as
  Record<string, [string, string]>;

async function openClassCrafter(page: Page) {
  await page.goto('/build');
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(emptyBuild);
  await page.locator('.import-submit').click();
  await openItemsAfterImport(page);
  await page.getByRole('button', { name: 'Craft item', exact: true }).click();
}

async function switchClassLanguage(page: Page, label: string) {
  await page.locator('.shell-settings-button').click();
  await page.locator('.shell-settings').getByRole('combobox').first().click();
  await page.getByRole('option', { name: label, exact: true }).click();
  await page.locator('.shell-settings').getByRole('button', { name: /^(Close|关闭|關閉)$/ }).click();
}

test('native class labels change languages while selection, draft text and saved item stay unchanged', async ({ page }) => {
  await installItemClassFixture(page, { scopes: { 'ItemClasses.Name': classEnglish } }, classRecords);
  await openClassCrafter(page);
  const crafter = page.locator('.item-crafter');
  const fields = crafter.locator('.item-crafter-fields [role="combobox"]');
  await expect(fields.nth(0)).toHaveText('Ring');
  await switchClassLanguage(page, '繁體中文');
  await expect(fields.nth(0)).toHaveText('測試類別0');
  await fields.nth(0).click();
  await page.getByRole('option', { name: '測試類別2', exact: true }).click();
  await expect(fields.nth(0)).toHaveText('測試類別2');
  await switchClassLanguage(page, 'English');
  await expect(fields.nth(0)).toHaveText('Warstaff');
  await expect(fields.nth(1)).toContainText('Quarterstaff');
  await fields.nth(0).click();
  await page.getByRole('option', { name: 'Staff', exact: true }).click();
  await expect(fields.nth(1)).toContainText('Ashen Staff');
  await fields.nth(0).click();
  await page.getByRole('option', { name: 'Ring', exact: true }).click();
  await fields.nth(1).click();
  await page.getByRole('option', { name: /^Iron Ring/ }).click();
  await crafter.getByRole('button', { name: 'Create draft', exact: true }).click();
  const text = crafter.locator('.item-draft-workbench textarea');
  const original = await text.inputValue();
  expect(original).toContain('Iron Ring');
  expect(original).not.toContain('測試類別');
  await switchClassLanguage(page, '繁體中文');
  await expect(text).toHaveValue(original);
  await switchClassLanguage(page, 'English');
  await crafter.getByRole('button', { name: 'Save to item library', exact: true }).click();
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-library')!).items.at(-1));
  expect(saved.text).toBe(original);
  await page.reload();
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-library')!).items.at(-1))).toEqual(saved);
});

test('a new class member prevents stale, missing and wrong-id translations from the text path', async ({ page }) => {
  await installItemClassFixture(page, { scopes: { 'ItemClasses.Name': classEnglish } }, {
    Ring: ['Old Rings', '不可顯示'], AnotherClass: ['Rings', '不可借用'], 'Body Armour': ['Body Armours', '測試整名'],
  });
  await openClassCrafter(page);
  await switchClassLanguage(page, '繁體中文');
  const classes = page.locator('.item-crafter-fields [role="combobox"]').first();
  await expect(classes).toContainText('Ring');
  await expect(classes).not.toContainText(/不可顯示|不可借用/);
  await classes.click();
  await page.getByRole('option', { name: '測試整名', exact: true }).click();
  await expect(classes).toHaveText('測試整名');
  await switchClassLanguage(page, 'English');
  await expect(classes).toHaveText('Body Armour');
});

test('the official old pack keeps its fixed Body Armour label through actual name translation', async ({ page }) => {
  await openClassCrafter(page);
  const classes = page.locator('.item-crafter-fields [role="combobox"]').first();
  await classes.click();
  await page.getByRole('option', { name: 'Body Armour', exact: true }).click();
  await switchClassLanguage(page, '简体中文');
  await expect(classes).toHaveText('胸甲');
  await expect(page.locator('.item-crafter-fields [role="combobox"]').nth(1)).not.toBeEmpty();
  await switchClassLanguage(page, '繁體中文');
  await expect(classes).toHaveText('胸甲');
  await switchClassLanguage(page, 'English');
  await expect(classes).toHaveText('Body Armour');
});
