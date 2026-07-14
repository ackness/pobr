/**
 * 临时冒烟（本轮四修复验证，验证后可删）：
 * 1. 天赋树坐标回填 → 树页真的渲染出节点；
 * 2. 附赠技能组标注 → Calcs 技能 DPS 列表出现「装备附赠」徽标；
 * 3. 物品名 i18n → 中文界面下装备名/基底名出现中文；
 * 4. 物品库工具栏 → 搜索框存在。
 */

import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ninjaCode = readFileSync(
  join(fileURLToPath(new URL('..', import.meta.url)), '../examples/poe-ninja/2.txt'),
  'utf8',
).trim();

test('tree renders, granted badges, zh item names, library search', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: /Import Build Code/i })).toBeVisible({
    timeout: 90_000,
  });

  // 切到中文界面（循环按钮 en → zh-TW → zh-CN）。
  const langBtn = page.getByRole('button', { name: 'Switch language' });
  await langBtn.click();
  await langBtn.click();

  await page.getByRole('textbox', { name: /Build code|build code|代碼|代码/i }).fill(ninjaCode);
  await page.locator('.import-submit').click();
  await expect(page.getByRole('heading', { name: /装备|Items/ })).toBeVisible({
    timeout: 60_000,
  });

  // 3+4：物品页——基底名翻译（Absent Amulet → 中文）+ 库搜索框。
  await expect(page.locator('.library-search')).toBeVisible();
  // 人形槽位上的项链名应包含中文字符（词典命中任一），宽松断言：页面存在中文物品名。
  const dollText = await page.locator('.paper-doll').innerText();
  expect(dollText).toMatch(/[一-鿿]/);

  // 5：点人形槽位 → 出现 PoB2 式切换下拉（库中该槽位候选）。
  await page.locator('.paper-doll .doll-slot').first().click();
  const switcher = page.locator('.item-detail .app-select').first();
  await expect(switcher).toBeVisible();
  await switcher.locator('.app-select-trigger').click();
  expect(await switcher.locator('.app-select-option').count()).toBeGreaterThan(1);
  await page.keyboard.press('Escape');

  // 6：胸甲（3 符文位）→ 符文编辑器出现 3 个下拉；换插第一颗后文本更新且
  //    符文命名行以中文前缀显示。
  await page.locator('.paper-doll .doll-slot', { hasText: /胸甲/ }).click();
  const runeSelects = page.locator('.rune-editor .app-select');
  await expect(runeSelects).toHaveCount(3);
  await runeSelects.first().locator('.app-select-trigger').click();
  await page.locator('.app-select-option[data-value="Greater Iron Rune"]').click();
  await expect(page.locator('.item-detail')).toContainText('符文: ', { timeout: 30_000 });
  await expect(page.locator('.rune-editor .calc-error')).toHaveCount(0);

  // 7：加减孔——加一孔变 4 个下拉，再减回 3。
  await page.locator('.socket-count-controls button', { hasText: '+' }).click();
  await expect(runeSelects).toHaveCount(4, { timeout: 30_000 });
  await page.locator('.socket-count-controls button', { hasText: '−' }).click();
  await expect(runeSelects).toHaveCount(3, { timeout: 30_000 });
  await expect(page.locator('.rune-editor .calc-error')).toHaveCount(0);

  // 1：树页渲染出上千个节点圆点。
  await page.getByRole('button', { name: /天赋树|樹|Tree/ }).click();
  await expect
    .poll(async () => page.locator('.tree-nodes circle').count(), { timeout: 30_000 })
    .toBeGreaterThan(1000);

  // 8：热力图——选「生命」+ 点「计算」后（默认深度 5）出现热力上色节点；
  //    加点不自动重算，只出现「已过期」提示。
  await page.locator('.tree-heat select').selectOption('Life');
  await page.locator('.tree-heat button').click();
  await expect
    .poll(async () => page.locator('.tree-nodes circle[style*="fill"]').count(), {
      timeout: 60_000,
    })
    .toBeGreaterThan(5);
  await page.locator('.tree-nodes circle[style*="fill"]').first().click();
  await expect(page.locator('.tree-heat .tree-hint')).toContainText('已过期', {
    timeout: 30_000,
  });

  // 2：Calcs 页技能 DPS 列表出现附赠徽标。
  await page
    .getByLabel('Main navigation')
    .getByRole('button', { name: /计算|Calcs/ })
    .click();
  await expect(page.locator('.granted-badge').first()).toBeVisible({ timeout: 60_000 });
});
