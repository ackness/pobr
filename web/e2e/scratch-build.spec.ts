/**
 * 白手起 build 流程（PoB2 新建语义）：不导入任何 code——启动即有默认角色、
 * 侧边栏出数；切职业、改等级、树上点选加点均实时重算。
 */

import { expect, test } from '@playwright/test';

test('scratch build: class picker, level edit, tree allocation', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Character' })).toBeVisible({ timeout: 90_000 });

  // 默认空 build 已计算：侧边栏 Life 有值且非 0/—。
  const sidebar = page.getByRole('complementary', { name: 'Character stats' });
  const lifeValue = sidebar.locator('.stat-row', { hasText: 'Life' }).locator('dd').first();
  await expect(lifeValue).not.toHaveText('—');
  await expect(lifeValue).not.toHaveText('0');

  // 切职业 → 顶栏角色名跟随。
  await page.getByLabel('Class').selectOption('Witch');
  await expect(page.locator('.topbar-character')).toContainText('Witch', { timeout: 30_000 });

  // 改等级 → Life 变化（等级驱动基础生命）。
  const lifeBefore = await lifeValue.textContent();
  await page.getByLabel('Level').fill('90');
  await expect(lifeValue).not.toHaveText(lifeBefore!, { timeout: 30_000 });

  // 树上点一个节点 → 已加点计数变为 1。
  await page.getByRole('button', { name: 'Tree' }).click();
  await expect(page.locator('.tree-canvas svg')).toBeVisible({ timeout: 30_000 });
  await page.locator('.node').first().click({ force: true });
  await expect(page.locator('.tree-count')).toContainText('1 allocated', { timeout: 30_000 });

  // 手动添加技能：自定义选择器搜 Comet → 回车选中首项 → 新组出现 → Total DPS 出数。
  await page.getByRole('button', { name: 'Skills' }).click();
  const picker = page.getByLabel(/Search an active gem/);
  await picker.fill('Comet');
  await expect(page.locator('.gem-picker-item').first()).toBeVisible();
  await picker.press('Enter');
  await expect(page.locator('.skill-group')).toHaveCount(1, { timeout: 30_000 });
  const dpsValue = sidebar.locator('.stat-row', { hasText: 'Total DPS' }).locator('dd').first();
  await expect(dpsValue).not.toHaveText('0', { timeout: 30_000 });
  await expect(dpsValue).not.toHaveText('—');

  // 手动添加装备：人形布局点 Ring 1 槽位 → 编辑面板默认模板（+50 Life）→ Life 变化。
  const lifeBeforeItem = await lifeValue.textContent();
  await page.getByRole('button', { name: 'Items' }).click();
  await page.locator('.paper-doll').getByRole('button', { name: 'Ring 1' }).click();
  await page.locator('.item-detail').getByRole('button', { name: 'Apply' }).click();
  await expect(lifeValue).not.toHaveText(lifeBeforeItem!, { timeout: 30_000 });

  // 三语切换：EN → 繁 → 简（页签文案跟随，简繁字形区分）。
  await page.locator('.topbar-lang').click();
  await expect(page.getByRole('button', { name: '天賦樹' })).toBeVisible();
  await page.locator('.topbar-lang').click();
  await expect(page.getByRole('button', { name: '天赋树' })).toBeVisible();
  await page.locator('.topbar-lang').click();
  await expect(page.getByRole('button', { name: 'Tree' })).toBeVisible();
});
