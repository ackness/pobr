/**
 * E2E 冒烟：真实 demo build 走完「初始化 → 导入 build code →
 * 结果栏出数 → 页面浏览」主流程。前置：`pnpm build-wasm && pnpm sync-data
 * && pnpm build`（playwright.config 的 preview server 消费 dist/）。
 */

import { expect, test } from '@playwright/test';
import { openItemsAfterImport, pageLink, statRow, statValue } from './shell';
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
  await page.goto('/build');

  // 等待 wasm + 数据初始化完成（boot 屏消失，导入区出现）。
  await expect(page.getByRole('heading', { name: 'Import a build' })).toBeVisible({
    timeout: 90_000,
  });

  await page.getByRole('textbox', { name: 'Build code' }).fill(demoCode);
  await page.locator('.import-submit').click();

  // 计算完成 → 自动落到概览页；结果栏出现非零 EnergyShield，再进入 Items 页。
  await openItemsAfterImport(page);
  await expect(page.getByRole('heading', { name: 'Items', exact: true })).toBeVisible({ timeout: 60_000 });
  const sidebar = page.getByRole('complementary', { name: 'Character stats' });
  await expect(sidebar).toContainText('Energy Shield');
  const esValue = statValue(page, /^Energy Shield$/).first();
  await expect(statRow(page, /^Energy Shield$/)).toHaveCount(1);
  await expect(esValue).not.toHaveText('—');
  await expect(esValue).not.toHaveText('0');

  // Skills 页：技能组渲染 + 主技能标记。
  await pageLink(page, 'Skills').click();
  await expect(page.locator('.skill-group').first()).toBeVisible();
  await expect(page.locator('.skill-group.is-main')).toHaveCount(1);

  // Calcs 页：点开一项，明细出现在右侧详情里。
  await pageLink(page, 'Calcs').click();
  await page.locator('#breakdown-Life').click();
  await expect(page.locator('#calcs-detail .formula-final, #calcs-detail .breakdown-table').first()).toBeVisible();

  // Tree 页：SVG 渲染 + 已加点数量显示。
  await pageLink(page, 'Tree').click();
  await expect(page.locator('.tree-canvas svg')).toBeVisible({ timeout: 30_000 });
  await expect(page.locator('.node-allocated').first()).toBeVisible();
});
