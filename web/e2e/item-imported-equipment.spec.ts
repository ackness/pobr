import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { deflateSync } from 'node:zlib';
import { goTo } from './buildLibrary';
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
  const runes = page.locator('.rune-editor .rune-socket');
  await expect(runes).toHaveCount(3);
  const socket = runes.first().getByRole('combobox');
  await expect(socket).toContainText('完美钢铁符文');
  await socket.click();
  const picker = page.getByRole('dialog', { name: '胸甲 · 插槽 1', exact: true });
  await expect(picker).toContainText('当前增幅器');
  const search = picker.getByRole('searchbox');
  await expect(search).toBeFocused();
  await search.fill('高级钢铁');
  await picker.getByRole('radio', { name: /^高级钢铁符文/ }).check();
  await expect(picker.locator('.rune-picker-preview')).toContainText('高级钢铁符文');
  await picker.getByRole('button', { name: '取消', exact: true }).click();
  await expect(picker).toBeHidden();
  await expect(socket).toContainText('完美钢铁符文');

  await socket.click();
  await search.fill('高级钢铁');
  await picker.getByRole('radio', { name: /^高级钢铁符文/ }).check();
  await picker.getByRole('button', { name: '替换增幅器', exact: true }).click();
  await expect(socket).toContainText('高级钢铁符文', { timeout: 30_000 });
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

test('a helmet imported from WeGame groups its desecrated resistance suffix with the natural suffixes', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 1250 });
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: 85, class_name: 'Sorceress' },
    equipments: [{ inventoryId: 'Helm', baseType: '先祖冠冕', frameType: 2,
      name: '测试冠冕', ilvl: 80, corrupted: true,
      properties: [{ name: '品质', type: 6, values: [['+20%', 1]] }, { name: '能量护盾', type: 18, values: [['420', 1]] }],
      sockets: [{ type: 'rune' }, { type: 'rune' }],
      socketedItems: [{ socket: 0, baseType: 'Greater Iron Rune' }, { socket: 1, baseType: 'Greater Iron Rune' }],
      runeMods: ['36% increased Armour, Evasion and Energy Shield'],
      explicitMods: ['+44 to maximum Energy Shield', '93% increased Energy Shield', '+135 to maximum Life',
        '+31% to Fire Resistance', '+38% to Cold Resistance', '+14% 火焰与混沌抗性'] }],
    talent_tree: { hashes: [], quest_stats: [] }, jewel_data: '[]', skills: [],
  } }));
  await importBuild(page, 'https://www.wegame.com.cn/helper/poe2/#/share/SyntheticHelmetFixture');
  await setLanguage(page, 'zh-CN');
  await goTo(page, '装备', 'zh-CN');
  await page.locator('.doll-cell[data-slot="helmet"] .doll-slot').click();
  const detail = page.locator('.item-detail .item-display');
  const prefixes = detail.locator('[data-affix-group="prefix"]');
  const suffixes = detail.locator('[data-affix-group="suffix"]');
  await expect(prefixes.locator('.item-affix-tier')).toHaveText(['T3', 'T1', 'T2']);
  await expect(suffixes.locator('.item-affix-tier')).toHaveText(['T3', 'T2']);
  const resistance = suffixes.locator('[data-kind="explicit"]').filter({ hasText: '+14%' });
  await expect(resistance).toHaveAttribute('data-affix', 'suffix');
  await expect(resistance.locator('.item-affix-tier, [data-modifier-source], [data-mod-tag]')).toHaveCount(0);
  await expect(detail.locator('.item-affix-remainder')).toHaveCount(0);
  await expect(detail.locator('.item-extra-effects + .item-affix-group')).toBeVisible();
  await expect(detail.locator('.item-socketed-runes li')).toHaveCount(2);
  await detail.screenshot({ path: testInfo.outputPath('wegame-helmet-desktop.png') });
  await page.setViewportSize({ width: 390, height: 900 });
  await expect(resistance).toBeVisible();
  expect(await detail.evaluate(element => element.scrollWidth - element.clientWidth)).toBeLessThanOrEqual(1);
  await detail.screenshot({ path: testInfo.outputPath('wegame-helmet-mobile.png') });
});

test('WeGame equipment places base, enchantment and socket effects above catalyst affixes', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: 81, class_name: 'Deadeye' },
    equipments: [
      { inventoryId: 'Ring', x: 0, baseType: 'Gold Ring', frameType: 2, name: 'Frost Whorl', ilvl: 79,
        properties: [{ name: '品质（冰霜词缀）', type: 6, values: [['+20%', 1]] }],
        implicitMods: ['9% increased Rarity of Items found'],
        explicitMods: ['Adds 10 to 25 Physical Damage to Attacks', 'Adds 26 to 38 Cold damage to Attacks',
          '+16% to all Elemental Resistances', '+43% to Cold Resistance', '+35% to Lightning Resistance'],
        desecratedMods: ['30% increased Cold Damage'] },
      { inventoryId: 'Gloves', baseType: 'Runeforged Secured Wraps', frameType: 2, name: 'Synthetic Wraps', ilvl: 81,
        corrupted: true,
        sockets: [{ type: 'rune' }], socketedItems: [{ socket: 0, baseType: 'Perfect Body Rune' }],
        enchantMods: ['+1 to Maximum Frenzy Charges'], runeMods: ['+40 to maximum Life'],
        implicitMods: ['+12 to maximum Mana'], explicitMods: ['+74 to Evasion Rating'] },
    ],
    talent_tree: { hashes: [], quest_stats: [] }, jewel_data: '[]', skills: [],
  } }));
  await importBuild(page, 'https://www.wegame.com.cn/helper/poe2/#/share/SyntheticShareKey_123456');
  await setLanguage(page, 'zh-CN');
  await goTo(page, '装备', 'zh-CN');
  const detail = page.locator('.item-detail .item-display');
  await page.locator('.doll-cell[data-slot="ring1"] .doll-slot').click();
  await expect(detail.locator('.item-extra-effects [data-kind="implicit"]')).toContainText('9%');
  const prefixes = detail.locator('[data-affix-group="prefix"]');
  const suffixes = detail.locator('[data-affix-group="suffix"]');
  await expect(prefixes.locator('.item-affix-tier')).toHaveText(['T2', 'T1']);
  await expect(prefixes.locator('[data-mod-tag="cold"]')).toHaveText('冰霜');
  await expect(suffixes.locator('.item-affix-tier')).toHaveText(['T2', 'T3']);
  await expect(detail.locator('.item-affix-remainder [data-kind="explicit"]')).toHaveCount(2);
  await expect(detail.locator('.item-affix-remainder [data-modifier-source="desecrated"]')).toBeVisible();
  await expect(detail.locator('.item-extra-effects + .item-affix-group')).toBeVisible();
  await detail.screenshot({ path: testInfo.outputPath('wegame-ring-desktop.png') });
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(suffixes.locator('[data-mod-tag="cold"]')).toBeVisible();
  expect(await detail.evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true);
  await detail.screenshot({ path: testInfo.outputPath('wegame-ring-mobile.png') });

  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.locator('.doll-cell[data-slot="gloves"] .doll-slot').click();
  await page.locator('.shell-settings-button').click();
  await page.locator('.display-mode-choice').filter({ hasText: '游戏卡片' }).click();
  await page.locator('.shell-settings').getByRole('button', { name: '关闭', exact: true }).click();
  await expect(page.locator('.shell-settings')).toBeHidden();
  const effects = detail.locator('.item-extra-effects');
  await expect(detail).toHaveAttribute('data-item-display', 'game');
  await expect(detail.locator('.item-display-badges')).toContainText('已腐化');
  await expect(effects.locator('.item-socketed-runes li')).toHaveCount(1);
  await expect(effects.locator('[data-kind]')).toHaveCount(3);
  await expect(effects.locator('[data-kind="implicit"]')).toContainText('12');
  await expect(effects.locator('[data-kind="enchant"]')).toContainText('+1');
  await expect(effects.locator('[data-kind="rune"]')).toContainText('40');
  await expect(detail.locator('.item-extra-effects + .item-affix-group')).toBeVisible();
  await detail.screenshot({ path: testInfo.outputPath('wegame-gloves-game-card.png') });
});
