/**
 * E2E 冒烟（TODO.md 2.4）：真实 demo build 走完「初始化 → 导入 build code →
 * 侧边栏出数 → 页签浏览」主流程。前置：`npm run build-wasm && npm run sync-data
 * && npm run build`（playwright.config 的 preview server 消费 dist/）。
 */

import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const demoCode = readFileSync(
  join(
    fileURLToPath(new URL('..', import.meta.url)),
    '../examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt',
  ),
  'utf8',
).trim();

test('import build code and browse tabs', async ({ page }) => {
  await page.goto('/');

  // 等待 wasm + 数据初始化完成（boot 屏消失，导入页出现）。
  await expect(page.getByRole('heading', { name: /Import Build Code/i })).toBeVisible({
    timeout: 90_000,
  });

  await page.getByRole('textbox', { name: 'Build code' }).fill(demoCode);
  await page.locator('.import-submit').click();

  // 计算完成 → 自动切到 Items 页，侧边栏出现非零 EnergyShield。
  await expect(page.getByRole('heading', { name: 'Items' })).toBeVisible({ timeout: 60_000 });
  const sidebar = page.getByRole('complementary', { name: 'Character stats' });
  await expect(sidebar).toContainText('Energy Shield');
  const esValue = sidebar.locator('.stat-row', { hasText: 'Energy Shield' }).locator('dd').first();
  await expect(esValue).not.toHaveText('—');
  await expect(esValue).not.toHaveText('0');

  // Skills 页：技能组渲染 + 主技能标记。
  await page.getByRole('button', { name: 'Skills' }).click();
  await expect(page.locator('.skill-group').first()).toBeVisible();
  await expect(page.locator('.skill-group.is-main')).toHaveCount(1);

  // Calcs 页：breakdown 项可展开。
  await page.getByRole('button', { name: 'Calcs' }).click();
  const firstBreakdown = page.locator('.breakdown-toggle').first();
  await firstBreakdown.click();
  await expect(page.locator('.breakdown-table').first()).toBeVisible();

  // Tree 页：SVG 渲染 + 已加点数量显示。
  await page.getByRole('button', { name: 'Tree' }).click();
  await expect(page.locator('.tree-canvas svg')).toBeVisible({ timeout: 30_000 });
  await expect(page.locator('.node-allocated').first()).toBeVisible();
});
