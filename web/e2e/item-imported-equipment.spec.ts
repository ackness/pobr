import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { deflateSync } from 'node:zlib';
import { choose, goTo } from './buildLibrary';
import { importBuild } from './skillsAndCalcs';
import { setLanguage } from './treePage';

const code = readFileSync(new URL('../../examples/poe-ninja/2.txt', import.meta.url), 'utf8').trim();

test('imported equipment has native bases, slot candidates and editable rune sockets', async ({ page }) => {
  await importBuild(page, code);
  await setLanguage(page, 'zh-CN');
  await goTo(page, '装备', 'zh-CN');
  await expect(page.getByRole('heading', { name: '装备', exact: true })).toBeVisible();
  await expect(page.locator('.library-search')).toBeVisible();

  // The prepared zh-CN BaseItemTypes.Name row translates Absent Amulet to 失神项链.
  // Slot labels alone must not satisfy the item-name localization regression.
  const amulet = page.locator('.doll-cell[data-slot="amulet"] .doll-slot');
  await expect(amulet.locator('.doll-item-base')).toHaveText('失神项链');
  await amulet.click();
  await expect(amulet).toHaveAttribute('aria-pressed', 'true');
  const switcher = page.locator('.item-detail').getByRole('combobox', { name: '切换装备', exact: true });
  await switcher.click();
  await expect(page.getByRole('option', { name: /失神项链/ })).toBeVisible();
  expect(await page.getByRole('option').count()).toBeGreaterThan(1);
  await page.keyboard.press('Escape');

  const body = page.locator('.doll-cell[data-slot="bodyarmour"] .doll-slot');
  await body.click();
  await expect(body).toHaveAttribute('aria-pressed', 'true');
  const runes = page.locator('.rune-editor .app-select');
  await expect(runes).toHaveCount(3);
  await choose(page, runes.first().getByRole('combobox'), /^高级钢铁符文/);
  await expect(page.locator('.item-detail')).toContainText('符文: ', { timeout: 30_000 });
  await expect(page.locator('.rune-editor .calc-error')).toHaveCount(0);

  await page.locator('.socket-count-controls').getByRole('button', { name: '加孔', exact: true }).click();
  await expect(runes).toHaveCount(4, { timeout: 30_000 });
  await page.locator('.socket-count-controls').getByRole('button', { name: '减孔', exact: true }).click();
  await expect(runes).toHaveCount(3, { timeout: 30_000 });
  await expect(page.locator('.rune-editor .calc-error')).toHaveCount(0);
});

test('equipment details show a tagged catalyst roll from the real engine', async ({ page }, testInfo) => {
  // Gold Ring's top raw cold roll is 21–24 to 32–37. The imported values include quality.
  const item = `Rarity: RARE
Frost Whorl
Gold Ring
Item Level: 79
Quality (Cold Modifiers): +20%
Implicits: 0
{tags:cold,damage,attack}Adds 26 to 38 Cold damage to Attacks
{crafted}+12% to Fire Resistance`;
  const xml = `<PathOfBuilding2><Build level="81" className="Ranger"/>
<Items activeItemSet="1"><Item id="1">${item}</Item>
<ItemSet id="1"><Slot name="Ring 1" itemId="1"/></ItemSet></Items></PathOfBuilding2>`;
  await importBuild(page, deflateSync(xml).toString('base64url'));
  await setLanguage(page, 'zh-CN');
  await goTo(page, '装备', 'zh-CN');
  await page.locator('.doll-cell[data-slot="ring1"] .doll-slot').click();
  const detail = page.locator('.item-detail .item-display');
  const modifiers = detail.locator('[data-kind="explicit"]');
  await expect(modifiers).toHaveCount(2);
  const cold = modifiers.nth(0);
  await expect(cold).toContainText('26');
  await expect(cold).toContainText('38');
  await expect(cold).toHaveAttribute('data-affix', 'prefix');
  await expect(cold.getByText('T1', { exact: true })).toBeVisible();
  await expect(cold.locator('[data-mod-tag="cold"]')).toHaveText('冰霜');
  await expect(cold.locator('[data-mod-tag="damage"]')).toHaveText('伤害');
  const crafted = modifiers.nth(1);
  await expect(crafted).toContainText('12');
  await expect(crafted.locator('[data-modifier-source="crafted"]')).toHaveText('工艺');
  await expect(crafted.locator('.item-affix-tier')).toHaveCount(0);
  await expect(detail.locator('.item-properties')).toContainText('品质（冰霜词缀）');
  await expect(detail.locator('.item-properties')).toContainText('+20%');
  await expect(detail.getByText('未分类词缀', { exact: true })).toHaveCount(0);
  await page.setViewportSize({ width: 390, height: 900 });
  await expect(detail.locator('[data-mod-tag="cold"]')).toBeVisible();
  await detail.screenshot({ path: testInfo.outputPath('ring-modifiers.png') });
});
