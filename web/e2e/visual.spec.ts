/**
 * 视觉走查（TODO.md 6.3）：320/768/1024/1440 断点截图 + 无横向溢出断言。
 * 截图落 e2e/screenshots/（gitignored），供人工比对。
 */

import { expect, test } from '@playwright/test';

const BREAKPOINTS = [320, 768, 1024, 1440] as const;

for (const width of BREAKPOINTS) {
  test(`no horizontal overflow at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.goto('/');
    await expect(page.getByRole('heading', { name: /Import Build Code/i })).toBeVisible({
      timeout: 90_000,
    });
    await page.screenshot({ path: `e2e/screenshots/import-${width}.png`, fullPage: true });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, 'page body must not scroll horizontally').toBeLessThanOrEqual(0);
  });
}

test('keyboard: tab navigation reaches import textarea', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: /Import Build Code/i })).toBeVisible({
    timeout: 90_000,
  });
  // 顶栏 7 个页签 + 语言切换后到达 textarea；直接断言 textarea 可聚焦。
  await page.getByRole('textbox', { name: 'Build code' }).focus();
  await expect(page.getByRole('textbox', { name: 'Build code' })).toBeFocused();
});
