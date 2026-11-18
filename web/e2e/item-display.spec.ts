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
  await expect(page.locator('.shell-calc-error')).toHaveCount(0);
  await pageLink(page, 'Items').click();
  await page.locator('.doll-cell[data-slot="weapon1"] button').click();
  await expect(detail(page).locator('[data-kind="explicit"]')).toHaveCount(6);
});

test('settings switch all item details immediately, persist and leave the build and editor intact', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1700, height: 1600 });
  await expect(detail(page)).toHaveAttribute('data-item-display', 'grouped');
  const before = await page.evaluate(() => ({ saved: localStorage.getItem('pobr-v2-build-state'), calculations: window.itemDisplayProbe.calculations }));
  for (const [value, name] of [['compact', 'Compact tables'], ['game', 'Game cards'], ['grouped', 'Grouped details']]) {
    await openSettings(page);
    await page.locator('.display-mode-choice').filter({ hasText: name }).click();
    if (value === 'grouped') {
      await page.locator('.shell-settings').evaluate(el => el.scrollTop = 0);
      await page.locator('.shell-settings').screenshot({ path: testInfo.outputPath('settings.png') });
    }
    await closeSettings(page);
    await expect(detail(page)).toHaveAttribute('data-item-display', value);
    await expect(detail(page).locator('.item-value').filter({ hasText: /^14%$/ })).toBeVisible();
    const extras = await detail(page).locator('.item-extra-effects').boundingBox();
    const affixes = await detail(page).locator('.item-affix-group, .item-affix-table').boundingBox();
    expect(extras && affixes && extras.y + extras.height <= affixes.y).toBe(true);
    if (value === 'grouped') {
      const prefix = await detail(page).locator('[data-affix-group="prefix"]').boundingBox();
      const suffix = await detail(page).locator('[data-affix-group="suffix"]').boundingBox();
      expect(prefix && suffix && prefix.y === suffix.y && prefix.x + prefix.width <= suffix.x).toBe(true);
    }
    await detail(page).screenshot({ path: testInfo.outputPath(`${value}.png`) });
    expect(await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-preferences')!).displayMode)).toBe(value);
  }
  expect(await page.evaluate(() => ({ saved: localStorage.getItem('pobr-v2-build-state'), calculations: window.itemDisplayProbe.calculations }))).toEqual(before);
  // The library uses the same presentation preference.
  const row = page.locator('.items-library .library-row').filter({ hasText: 'Woe Rain' }).first();
  await row.locator('.library-row-toggle').click();
  await expect(row.locator('.item-display')).toHaveAttribute('data-item-display', 'grouped');
  await openSettings(page);
  await page.locator('.display-mode-choice').filter({ hasText: 'Game cards' }).click();
  await closeSettings(page);
  await expect(row.locator('.item-display')).toHaveAttribute('data-item-display', 'game');
  await detail(page).getByRole('tab', { name: 'Original text' }).click();
  await expect(detail(page).locator('.item-original-text')).toHaveText(await page.evaluate(() => window.itemDisplayProbe.text));
  // Reload the real shell with the same stored workspace and deterministic backend.
  await page.goto('/e2e/fixtures/item-display.html');
  await pageLink(page, 'Items').click();
  await page.locator('.doll-cell[data-slot="weapon1"] button').click();
  await expect(detail(page)).toHaveAttribute('data-item-display', 'game');
  await page.locator('.item-detail-header').getByRole('button', { name: 'Edit text (advanced)', exact: true }).click();
  const editor = page.getByRole('textbox', { name: 'weapon1 item text' });
  await editor.fill('Unapplied equipment draft');
  await openSettings(page);
  await page.locator('.display-mode-choice').filter({ hasText: 'Compact tables' }).click();
  await closeSettings(page);
  await expect(editor).toHaveValue('Unapplied equipment draft');
});

test('layouts preserve their reading order and wrap tags on narrow screens in both themes', async ({ page }, testInfo) => {
  for (const theme of ['Light', 'Dark']) {
    for (const [mode, label] of [['grouped', 'Grouped details'], ['compact', 'Compact tables'], ['game', 'Game cards']]) {
      await openSettings(page);
      await page.getByRole('button', { name: theme, exact: true }).click();
      await page.locator('.display-mode-choice').filter({ hasText: label }).click();
      await closeSettings(page);
      await page.setViewportSize({ width: 390, height: 900 });
      const quality = detail(page).locator('.item-properties > div').filter({ hasText: 'Quality (Cold Modifiers)' });
      await expect(quality.locator('dd')).toHaveText('+20%');
      const rows = detail(page).locator('[data-kind="explicit"]');
      const modifierOrder = [
        'Adds 12 to 228 Lightning Damage', 'Adds 69 to 85 Cold Damage', '+4.92% to Critical Hit Chance',
        '14% increased Attack Speed', '+2 to Level of all Attack Skills', 'Adds 65 to 106 Fire Damage',
      ];
      await expect(rows).toHaveCount(modifierOrder.length);
      await expect(rows).toContainText((mode === 'grouped' ? [0, 1, 5, 3, 2, 4] : [0, 1, 2, 3, 4, 5])
        .map(index => modifierOrder[index]));
      if (mode === 'grouped') {
        const prefix = await detail(page).locator('[data-affix-group="prefix"]').boundingBox();
        const suffix = await detail(page).locator('[data-affix-group="suffix"]').boundingBox();
        expect(prefix && suffix && prefix.x === suffix.x && prefix.y + prefix.height <= suffix.y).toBe(true);
      }
      await expect(detail(page).getByText('Unclassified', { exact: false })).toHaveCount(0);
      const unknown = rows.filter({ hasText: '+4.92% to Critical Hit Chance' });
      await expect(unknown.locator('.item-affix-side, .item-affix-tier, [data-mod-tag]')).toHaveCount(0);
      const cold = rows.filter({ hasText: 'Adds 69 to 85 Cold Damage' });
      await expect(cold).toHaveAttribute('data-affix', 'prefix');
      const prefixLabel = mode === 'grouped' ? detail(page).locator('[data-affix-group="prefix"]') : cold;
      await expect(prefixLabel.getByText('Prefix', { exact: true })).toBeVisible();
      await expect(cold.getByText('T2', { exact: true })).toBeVisible();
      const tags = cold.locator('[data-mod-tag]');
      await expect(tags).toHaveText(['Cold', 'Damage', 'Elemental', 'Attack', 'Projectile', 'Future tag with a long label']);
      expect(await tags.evaluateAll(elements => new Set(elements.map(el => el.getBoundingClientRect().top)).size)).toBeGreaterThan(1);
      await expect(rows.filter({ hasText: '14% increased Attack Speed' })).toHaveAttribute('data-affix', 'suffix');
      const fractured = rows.filter({ hasText: 'Adds 12 to 228 Lightning Damage' });
      await expect(fractured.locator('[data-modifier-source="fractured"]')).toHaveText('Fractured · locked');
      await expect(rows.filter({ hasText: '+2 to Level of all Attack Skills' }).locator('[data-modifier-source="crafted"]')).toHaveText('Crafted');
      await expect(fractured).toHaveAttribute('data-damage', 'lightning');
      const overflow = await detail(page).evaluate(root => {
        const right = root.getBoundingClientRect().right;
        return [...root.querySelectorAll('*')].some(el => el.getBoundingClientRect().width > 0 && el.getBoundingClientRect().right > right + 1);
      });
      expect(overflow).toBe(false);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
      if (theme === 'Dark') await cold.screenshot({ path: testInfo.outputPath(`narrow-${mode}-tags.png`) });
      await page.setViewportSize({ width: 1280, height: 900 });
    }
  }
});
