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
});
