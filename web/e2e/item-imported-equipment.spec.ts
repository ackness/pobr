import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { choose, goTo } from './buildLibrary';
import { importBuild } from './skillsAndCalcs';
import { setLanguage } from './treePage';

const code = readFileSync(new URL('../../examples/poe-ninja/2.txt', import.meta.url), 'utf8').trim();

test('imported equipment has native bases, slot candidates and editable rune sockets', async ({ page }) => {
  await importBuild(page, code);
  await setLanguage(page, 'zh-CN');
  await goTo(page, '装备', 'zh-CN');
  await expect(page.getByRole('heading', { name: '装备', exact: true })).toBeVisible();
  await expect(page.locator('.library-search')).toBeVisible();

  // The prepared zh-CN BaseItemTypes.Name row translates Absent Amulet to 失神项链.
  // Slot labels alone must not satisfy the item-name localization regression.
  const amulet = page.locator('.doll-cell[data-slot="amulet"] .doll-slot');
  await expect(amulet.locator('.doll-item-base')).toHaveText('失神项链');
  await amulet.click();
  await expect(amulet).toHaveAttribute('aria-pressed', 'true');
  const switcher = page.locator('.item-detail').getByRole('combobox', { name: '切换装备', exact: true });
  await switcher.click();
  await expect(page.getByRole('option', { name: /失神项链/ })).toBeVisible();
  expect(await page.getByRole('option').count()).toBeGreaterThan(1);
  await page.keyboard.press('Escape');

  const body = page.locator('.doll-cell[data-slot="bodyarmour"] .doll-slot');
  await body.click();
  await expect(body).toHaveAttribute('aria-pressed', 'true');
  const runes = page.locator('.rune-editor .app-select');
  await expect(runes).toHaveCount(3);
  await choose(page, runes.first().getByRole('combobox'), /^高级钢铁符文/);
  await expect(page.locator('.item-detail')).toContainText('符文: ', { timeout: 30_000 });
  await expect(page.locator('.rune-editor .calc-error')).toHaveCount(0);

  await page.locator('.socket-count-controls').getByRole('button', { name: '加孔', exact: true }).click();
  await expect(runes).toHaveCount(4, { timeout: 30_000 });
  await page.locator('.socket-count-controls').getByRole('button', { name: '减孔', exact: true }).click();
  await expect(runes).toHaveCount(3, { timeout: 30_000 });
  await expect(page.locator('.rune-editor .calc-error')).toHaveCount(0);
});
