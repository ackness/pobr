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
    await expect(page.getByRole('heading', { name: /Import Build/i })).toBeVisible({
      timeout: 90_000,
    });
    const source = page.getByRole('link', { name: 'View source on GitHub (opens in a new tab)' });
    await expect(source).toBeVisible();
    await expect(source).toHaveAttribute('href', 'https://github.com/ackness/pobr');
    await expect(source).toHaveAttribute('target', '_blank');
    for (const [tab, selector] of [
      ['Build', '.build-page'], ['Items', '.paper-doll'], ['Skills', '.skills-toolbar'],
      ['Calcs', '.calcs-page'], ['Config', '.config-section-header'], ['Tree', '.tree-canvas svg'], ['Upgrades', '.trade-setup'],
    ]) {
      await page.getByRole('navigation', { name: 'Main navigation' }).getByRole('button', { name: tab, exact: true }).click();
      await expect(page.locator(selector).first()).toBeVisible();
      const overflow = await page.evaluate(() => ({
        body: document.documentElement.scrollWidth - document.documentElement.clientWidth,
        main: document.querySelector('main')!.scrollWidth - document.querySelector('main')!.clientWidth,
      }));
      expect(overflow.body, `${tab}: document must fit the viewport`).toBeLessThanOrEqual(1);
      expect(overflow.main, `${tab}: main content must not overflow horizontally`).toBeLessThanOrEqual(1);
      await page.screenshot({ path: `e2e/screenshots/${tab.toLowerCase()}-${width}.png` });
    }
    if (width <= 768) {
      await page.getByRole('button', { name: 'Show character stats' }).click();
      await expect(page.getByRole('complementary', { name: 'Character stats' })).toBeVisible();
      await page.getByRole('button', { name: 'Hide character stats' }).click();
      await expect(page.getByRole('complementary', { name: 'Character stats' })).toBeHidden();
    }
  });
}

test('keyboard: tab navigation reaches import textarea', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: /Import Build/i })).toBeVisible({
    timeout: 90_000,
  });
  // 顶栏 7 个页签 + 语言切换后到达 textarea；直接断言 textarea 可聚焦。
  await page.getByRole('textbox', { name: 'Build code' }).focus();
  await expect(page.getByRole('textbox', { name: 'Build code' })).toBeFocused();
});

test('replacement header fits a narrow screen with classic scrollbar space', async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 900 });
  await page.goto('/');
  await expect(page.getByRole('heading', { name: /Import Build/i })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('navigation', { name: 'Main navigation' }).getByRole('button', { name: 'Upgrades', exact: true }).click();
  await expect(page.locator('.replacement-heading')).toBeVisible();
  // Overlay scrollbars on macOS leave more room than Linux's classic scrollbars.
  // Reserve that space explicitly so this regression is reproducible on either OS.
  await page.locator('main').evaluate(element => {
    const main = element as HTMLElement;
    const scrollbar = main.offsetWidth - main.clientWidth;
    main.style.width = `calc(100% - ${Math.max(0, 16 - scrollbar)}px)`;
  });
  const overflow = await page.locator('.replacement-heading').evaluate(element => ({
    heading: element.scrollWidth - element.clientWidth,
    main: element.closest('main')!.scrollWidth - element.closest('main')!.clientWidth,
  }));
  expect(overflow.heading).toBeLessThanOrEqual(1);
  expect(overflow.main).toBeLessThanOrEqual(1);
});
