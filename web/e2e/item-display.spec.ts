import { expect, test, type Page } from '@playwright/test';
import { deflateSync } from 'node:zlib';
import { pageLink } from './shell';

declare global { interface Window { itemDisplayProbe: { calculations: number; text: string } } }
const code = deflateSync('<PathOfBuilding2><Build level="81" className="Ranger"/><Items/></PathOfBuilding2>').toString('base64url');
const detail = (page: Page) => page.locator('.item-detail .item-display');
const openSettings = (page: Page) => page.locator('.shell-settings-button').click();
const closeSettings = async (page: Page) => {
  await page.locator('.shell-settings').getByRole('button', { name: /^(Close|关闭|關閉)$/ }).click();
  await expect(page.locator('.shell-settings')).not.toBeVisible();
};

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    if (!localStorage.getItem('pobr-v2-preferences')) localStorage.setItem('pobr-v2-preferences', JSON.stringify({ lang: 'en-US', theme: 'dark' }));
  });
  await page.goto('/e2e/fixtures/item-display.html');
  await expect(page.getByRole('textbox', { name: 'Build code', exact: true })).toBeVisible();
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();
  await expect(page.locator('#ov-title')).toBeVisible();
  await pageLink(page, 'Items').click();
  await page.locator('.doll-cell[data-slot="weapon1"] button').click();
  await expect(detail(page).locator('[data-affix="prefix"]')).toHaveCount(3);
});

test('settings switch all item details immediately, persist and leave the build and editor intact', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1700, height: 1600 });
  await expect(detail(page)).toHaveAttribute('data-item-display', 'grouped');
  const before = await page.evaluate(() => ({ saved: localStorage.getItem('pobr-v2-build-state'), calculations: window.itemDisplayProbe.calculations }));
  for (const [value, name] of [['compact', 'Compact table'], ['game', 'Game item card'], ['grouped', 'Grouped details']]) {
    await openSettings(page);
    await page.getByRole('combobox', { name: 'Equipment display', exact: true }).click();
    await page.getByRole('option', { name, exact: true }).click();
    if (value === 'grouped') {
      await page.locator('.shell-settings').evaluate(el => el.scrollTop = 0);
      await page.locator('.shell-settings').screenshot({ path: testInfo.outputPath('settings.png') });
    }
    await closeSettings(page);
    await expect(detail(page)).toHaveAttribute('data-item-display', value);
    await expect(detail(page).locator('.item-value').filter({ hasText: /^14%$/ })).toBeVisible();
    await detail(page).screenshot({ path: testInfo.outputPath(`${value}.png`) });
    expect(await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-preferences')!).itemDisplay)).toBe(value);
  }
  expect(await page.evaluate(() => ({ saved: localStorage.getItem('pobr-v2-build-state'), calculations: window.itemDisplayProbe.calculations }))).toEqual(before);
  // The library uses the same presentation preference.
  const row = page.locator('.items-library .library-row').filter({ hasText: 'Woe Rain' }).first();
  await row.locator('.library-row-toggle').click();
  await expect(row.locator('.item-display')).toHaveAttribute('data-item-display', 'grouped');
  await openSettings(page);
  await page.getByRole('combobox', { name: 'Equipment display', exact: true }).click();
  await page.getByRole('option', { name: 'Game item card', exact: true }).click();
  await closeSettings(page);
  await expect(row.locator('.item-display')).toHaveAttribute('data-item-display', 'game');
  await detail(page).getByRole('tab', { name: 'Original text' }).click();
  await expect(detail(page).locator('.item-original-text')).toHaveText(await page.evaluate(() => window.itemDisplayProbe.text));
  // Reload the real shell with the same stored workspace and deterministic backend.
  await page.goto('/e2e/fixtures/item-display.html');
  await pageLink(page, 'Items').click();
  await page.locator('.doll-cell[data-slot="weapon1"] button').click();
  await expect(detail(page)).toHaveAttribute('data-item-display', 'game');
  await page.locator('.item-detail-header').getByRole('button', { name: 'Edit', exact: true }).click();
  const editor = page.getByRole('textbox', { name: 'weapon1 item text' });
  await editor.fill('Unapplied equipment draft');
  await openSettings(page);
  await page.getByRole('combobox', { name: 'Equipment display', exact: true }).click();
  await page.getByRole('option', { name: 'Compact table', exact: true }).click();
  await closeSettings(page);
  await expect(editor).toHaveValue('Unapplied equipment draft');
});

test('all layouts retain unknown lines and fit a narrow viewport in light and dark themes', async ({ page }) => {
  for (const theme of ['Light', 'Dark']) {
    for (const label of ['Grouped details', 'Compact table', 'Game item card']) {
      await openSettings(page);
      await page.getByRole('button', { name: theme, exact: true }).click();
      await page.getByRole('combobox', { name: 'Equipment display', exact: true }).click();
      await page.getByRole('option', { name: label, exact: true }).click();
      await closeSettings(page);
      await page.setViewportSize({ width: 390, height: 900 });
      await expect(detail(page)).toContainText('+4.92%');
      await expect(detail(page)).toContainText('+2');
      await expect(detail(page).getByText('Unclassified', { exact: false }).first()).toBeVisible();
      const overflow = await detail(page).evaluate(root => {
        const right = root.getBoundingClientRect().right;
        return [...root.querySelectorAll('*')].some(el => el.getBoundingClientRect().width > 0 && el.getBoundingClientRect().right > right + 1);
      });
      expect(overflow).toBe(false);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
      await page.setViewportSize({ width: 1280, height: 900 });
    }
  }
});
