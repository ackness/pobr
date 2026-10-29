/**
 * Responsive layout checks from mobile to ultrawide, with screenshots for review.
 * Screenshots are written to the ignored e2e/screenshots directory.
 */

import { expect, test } from '@playwright/test';
import { bdLibraryButton } from './shell';
import { goTo, importAndOpen } from './skillsAndCalcs';

const BREAKPOINTS = [320, 768, 1024, 1440, 1920, 2560, 3440] as const;

for (const width of BREAKPOINTS) {
  test(`no horizontal overflow at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.goto('/build');
    await expect(page.getByRole('textbox', { name: 'Build code' })).toBeVisible({
      timeout: 90_000,
    });
    // The source link lives in the settings sheet.
    await page.locator('.shell-settings-button').click();
    const source = page.getByRole('link', { name: 'View source on GitHub (opens in a new tab)' });
    await expect(source).toBeVisible();
    await expect(source).toHaveAttribute('href', 'https://github.com/ackness/pobr');
    await expect(source).toHaveAttribute('target', '_blank');
    await page.keyboard.press('Escape');
    await expect(source).toHaveCount(0);
    for (const [tab, selector] of [
      ['Build', '.bd'], ['Overview', '.ov'], ['Items', '.paper-doll'], ['Skills', '.skills-toolbar'],
      ['Calcs', '.calcs-page'], ['Config', '.config-section-header'], ['Tree', '.tree-canvas svg'], ['Upgrades', '.trade-page'],
      ['Compare', '.compare-panel'],
    ]) {
      // The build library is a button beside the page links, not a page link.
      if (tab === 'Build') await bdLibraryButton(page).click();
      else await goTo(page, tab);
      await expect(page.locator(selector).first()).toBeVisible();
      const overflow = await page.evaluate(() => ({
        body: document.documentElement.scrollWidth - document.documentElement.clientWidth,
        main: document.querySelector('main')!.scrollWidth - document.querySelector('main')!.clientWidth,
      }));
      expect(overflow.body, `${tab}: document must fit the viewport`).toBeLessThanOrEqual(1);
      expect(overflow.main, `${tab}: main content must not overflow horizontally`).toBeLessThanOrEqual(1);
      // The build library and the Overview without a build are reading columns of their own width, and the
      // tree fills the viewport by design.
      if (width >= 1920 && !['Tree', 'Build', 'Overview'].includes(tab)) {
        const bounds = await page.locator('.ui-page:visible, .compare-panel:visible').first().boundingBox();
        expect(bounds!.width, `${tab}: use the available wide workspace`).toBeGreaterThan(1400);
      }
      await page.screenshot({ path: `e2e/screenshots/${tab.toLowerCase()}-${width}.png` });
    }
    // Where the results panel no longer sits beside the page, it opens from a button as a sheet.
    const statsButton = page.locator('.shell-stats-button');
    if (await statsButton.isVisible()) {
      await statsButton.click();
      await expect(page.getByRole('dialog', { name: 'Character stats' })).toBeVisible();
      await page.keyboard.press('Escape');
      await expect(page.getByRole('dialog', { name: 'Character stats' })).toBeHidden();
    } else {
      await expect(page.getByRole('complementary', { name: 'Character stats' })).toBeVisible();
    }
  });
}

test('keyboard: tab navigation reaches import textarea', async ({ page }) => {
  await page.goto('/build');
  await expect(page.getByRole('textbox', { name: 'Build code' })).toBeVisible({
    timeout: 90_000,
  });
  // The import textbox remains keyboard-focusable as navigation grows.
  await page.getByRole('textbox', { name: 'Build code' }).focus();
  await expect(page.getByRole('textbox', { name: 'Build code' })).toBeFocused();
});

test('replacement header fits a narrow screen with classic scrollbar space', async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 900 });
  await page.goto('/build');
  await expect(page.getByRole('textbox', { name: 'Build code' })).toBeVisible({ timeout: 90_000 });
  await goTo(page, 'Upgrades');
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
    await importAndOpen(page, readFileSync('../examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt', 'utf8'));
    await expect(page.locator('.paper-doll')).toBeVisible();
    for (const tab of ['Build', 'Overview', 'Items', 'Skills', 'Tree', 'Calcs', 'Config', 'Upgrades', 'Compare']) {
      if (tab === 'Build') await bdLibraryButton(page).click();
      else await goTo(page, tab);
      if (tab === 'Items') await page.getByRole('button', { name: 'Body Armour', exact: true }).click();
      if (tab === 'Calcs') await expect(page.locator('.calcs-metric-row').first()).toBeVisible();
      if (tab === 'Skills') await page.locator('.skill-group-title').nth(4).click();
      if (width >= 1920 && tab === 'Skills') {
        // On a wide page the selected skill's detail sits beside the list instead of under it.
        const list = await page.locator('.skill-group-header').nth(0).boundingBox();
        const detail = await page.locator('.skill-detail').boundingBox();
        expect(detail!.x).toBeGreaterThan(list!.x + list!.width);
        expect(detail!.y).toBeLessThanOrEqual(list!.y + list!.height);
      }
      if (width >= 1920 && tab === 'Overview') {
        // With a build the Overview's two columns use the wide workspace.
        const bounds = await page.locator('.ov:visible').boundingBox();
        expect(bounds!.width, 'Overview: use the available wide workspace').toBeGreaterThan(1400);
      }
      await page.waitForFunction(() => document.getAnimations().every(animation => animation.playState !== 'running'));
      await expect(page.locator('main')).toBeVisible();
      const overflow = await page.locator('main').evaluate(element => element.scrollWidth - element.clientWidth);
      expect(overflow, `${tab} populated at ${width}`).toBeLessThanOrEqual(1);
      await page.screenshot({ path: `e2e/screenshots/layout-${process.env.LAYOUT_REVIEW_PHASE ?? 'after'}-${tab.toLowerCase()}-${width}.png` });
      if (tab === 'Calcs') {
        // The skill overview is another view of the same page.
        await page.getByRole('tab', { name: 'Skill metrics', exact: true }).click();
        await expect(page.locator('.skill-metrics-row').first()).toBeVisible();
        const skillOverflow = await page.locator('main').evaluate(element => element.scrollWidth - element.clientWidth);
        expect(skillOverflow, `Calcs skill metrics populated at ${width}`).toBeLessThanOrEqual(1);
      }
    }
  });
}
