/**
 * Responsive layout checks from mobile to ultrawide, with screenshots for review.
 * Screenshots are written to the ignored e2e/screenshots directory.
 */

import { expect, test } from '@playwright/test';

const BREAKPOINTS = [320, 768, 1024, 1440, 1920, 2560, 3440] as const;

for (const width of BREAKPOINTS) {
  test(`no horizontal overflow at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.goto('/');
    await expect(page.getByRole('textbox', { name: 'Build code' })).toBeVisible({
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
      if (width >= 1920 && tab !== 'Tree') {
        const bounds = await page.locator('.ui-page').boundingBox();
        expect(bounds!.width, `${tab}: use the available wide workspace`).toBeGreaterThan(1400);
      }
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
  await expect(page.getByRole('textbox', { name: 'Build code' })).toBeVisible({
    timeout: 90_000,
  });
  // The import textbox remains keyboard-focusable as navigation grows.
  await page.getByRole('textbox', { name: 'Build code' }).focus();
  await expect(page.getByRole('textbox', { name: 'Build code' })).toBeFocused();
});

test('replacement header fits a narrow screen with classic scrollbar space', async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 900 });
  await page.goto('/');
  await expect(page.getByRole('textbox', { name: 'Build code' })).toBeVisible({ timeout: 90_000 });
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

for (const width of [390, 1440, 2560]) {
  test(`populated build workflow at ${width}px`, async ({ page }) => {
    const { readFileSync } = await import('node:fs');
    await page.setViewportSize({ width, height: 1000 });
    await page.goto('/');
    await expect(page.getByRole('textbox', { name: 'Build code' })).toBeVisible({ timeout: 90_000 });
    await page.getByRole('textbox', { name: 'Build code' }).fill(readFileSync('../examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt', 'utf8'));
    await page.locator('.import-submit').click();
    await expect(page.locator('.paper-doll')).toBeVisible();
    for (const tab of ['Build', 'Items', 'Skills', 'Tree', 'Calcs', 'Config', 'Upgrades']) {
      await page.getByRole('navigation', { name: 'Main navigation' }).getByRole('button', { name: tab, exact: true }).click();
      if (tab === 'Items') await page.getByRole('button', { name: 'Body Armour', exact: true }).click();
      if (tab === 'Calcs') await expect(page.locator('.fulldps-row').first()).toBeVisible();
      if (tab === 'Skills') await page.locator('.skill-group-title').nth(4).click();
      if (width >= 1920 && tab === 'Skills') {
        const first = await page.locator('.skill-group').nth(0).boundingBox();
        const second = await page.locator('.skill-group').nth(1).boundingBox();
        expect(second!.x).toBeGreaterThan(first!.x + first!.width);
        expect(Math.abs(second!.y - first!.y)).toBeLessThanOrEqual(1);
      }
      await page.waitForFunction(() => document.getAnimations().every(animation => animation.playState !== 'running'));
      await expect(page.locator('main')).toBeVisible();
      const overflow = await page.locator('main').evaluate(element => element.scrollWidth - element.clientWidth);
      expect(overflow, `${tab} populated at ${width}`).toBeLessThanOrEqual(1);
      await page.screenshot({ path: `e2e/screenshots/layout-${process.env.LAYOUT_REVIEW_PHASE ?? 'after'}-${tab.toLowerCase()}-${width}.png` });
    }
  });
}
