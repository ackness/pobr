/**
 * 实时保存与恢复：每次编辑自动写入 localStorage，刷新后完整恢复
 * （角色等级 / 笔记 / 界面语言页签偏好）；内置配置目录可搜索可开关。
 */

import { expect, test } from '@playwright/test';

test('edits persist across reload; builtin config toggles recalc', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Character' })).toBeVisible({ timeout: 90_000 });

  // 编辑等级 → 即改即存。
  await page.getByLabel('Level').fill('90');
  const sidebar = page.getByRole('complementary', { name: 'Character stats' });
  const lifeValue = sidebar.locator('.stat-row', { hasText: 'Life' }).locator('dd').first();
  await expect(lifeValue).not.toHaveText('—', { timeout: 30_000 });
  const lifeAt90 = await lifeValue.textContent();

  // 笔记逐键保存。
  await page.getByRole('button', { name: 'Notes' }).click();
  await page.getByRole('textbox', { name: 'Notes' }).fill('my build notes 我的笔记');

  // 内置配置：搜索定位 + 勾选一个开关 → 触发重算（busy 出现后消退，无错误）。
  await page.getByRole('button', { name: 'Config' }).click();
  await page.getByLabel('Search config options…').fill('Onslaught');
  const firstCheck = page.locator('.config-item input[type="checkbox"]').first();
  await expect(firstCheck).toBeVisible();
  await firstCheck.check();
  await expect(page.locator('.calc-error')).toHaveCount(0);

  // 刷新：等级 / 笔记 / 配置覆盖 / 当前页签全部恢复。
  await page.reload();
  await expect(page.getByRole('heading', { name: 'Configuration' })).toBeVisible({
    timeout: 90_000,
  });
  await page.getByLabel('Search config options…').fill('Onslaught');
  await expect(page.locator('.config-item.is-overridden input[type="checkbox"]').first()).toBeChecked();

  await page.getByRole('button', { name: 'Notes' }).click();
  await expect(page.getByRole('textbox', { name: 'Notes' })).toHaveValue(
    'my build notes 我的笔记',
  );

  await page.getByRole('button', { name: 'Build' }).click();
  await expect(page.getByLabel('Level')).toHaveValue('90');
  await expect(lifeValue).toHaveText(lifeAt90!, { timeout: 30_000 });
});
