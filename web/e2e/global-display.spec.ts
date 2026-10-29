import { expect, test, type Page } from '@playwright/test';
import { deflateSync } from 'node:zlib';
import { pageLink } from './shell';

const code = deflateSync('<PathOfBuilding2><Build level="81" className="Ranger"/><Items/></PathOfBuilding2>').toString('base64url');
const openSettings = (page: Page) => page.locator('.shell-settings-button').click();
const closeSettings = (page: Page) => page.locator('.shell-settings').getByRole('button', { name: /^(Close|关闭|關閉)$/ }).click();
const chooseMode = (page: Page, value: string) => page.locator(`.display-mode-choice:has(input[value="${value}"])`).click();

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    if (!localStorage.getItem('pobr-v2-preferences')) localStorage.setItem('pobr-v2-preferences', JSON.stringify({ lang: 'en-US', itemDisplay: 'compact' }));
  });
  await page.goto('/e2e/fixtures/item-display.html');
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();
  await expect(page.locator('#ov-title')).toBeVisible();
  await expect(page.locator('.shell-progress')).not.toHaveAttribute('data-active');
});

test('global modes, page overrides and languages preserve the saved build and current skill', async ({ page }) => {
  await page.setViewportSize({ width: 1500, height: 1100 });
  await pageLink(page, 'Skills').click();
  await expect(page.locator('.skill-detail')).toBeVisible();
  await expect(page.locator('main')).toHaveAttribute('data-display-mode', 'compact');
  const selected = await page.locator('.skill-detail-title').innerText();
  const before = await page.evaluate(() => ({ saved: localStorage.getItem('pobr-v2-build-state'), calculations: window.itemDisplayProbe.calculations }));
  for (const mode of ['grouped', 'game', 'compact']) {
    await openSettings(page);
    await chooseMode(page, mode);
    await closeSettings(page);
    await expect(page.locator('main')).toHaveAttribute('data-display-mode', mode);
    await expect(page.locator('.skill-detail-title')).toHaveText(selected);
  }
  expect(await page.evaluate(() => ({ saved: localStorage.getItem('pobr-v2-build-state'), calculations: window.itemDisplayProbe.calculations }))).toEqual(before);
  await openSettings(page);
  await chooseMode(page, 'grouped');
  await page.locator('.display-page-overrides > summary').click();
  await page.locator('#display-page-skills').click();
  await page.getByRole('option', { name: 'Game cards', exact: true }).click();
  await closeSettings(page);
  await expect(page.locator('main')).toHaveAttribute('data-display-mode', 'game');
  await pageLink(page, 'Items').click();
  await expect(page.locator('main')).toHaveAttribute('data-display-mode', 'grouped');
  await page.goto('/e2e/fixtures/item-display.html');
  await pageLink(page, 'Skills').click();
  await expect(page.locator('main')).toHaveAttribute('data-display-mode', 'game');
  for (const language of ['简体中文', '繁體中文', 'English']) {
    await openSettings(page);
    await page.locator('#shell-language').click();
    await page.getByRole('option', { name: language, exact: true }).click();
    await closeSettings(page);
    await expect(page.locator('main')).toHaveAttribute('data-display-mode', 'game');
    await expect(page.locator('.skill-detail input[type="number"]').first()).toBeVisible();
  }
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!))).toEqual(JSON.parse(before.saved!));
  await openSettings(page);
  await page.locator('.display-page-overrides > summary').click();
  await page.getByRole('button', { name: 'Use global setting on every page', exact: true }).click();
  await closeSettings(page);
  await expect(page.locator('main')).toHaveAttribute('data-display-mode', 'grouped');
});

test('mobile skills keep the selected editor first and let players reveal other groups', async ({ page }) => {
  await pageLink(page, 'Skills').click();
  await page.setViewportSize({ width: 390, height: 900 });
  const selected = page.locator('.skill-group.is-selected > .skill-group-header');
  const others = page.locator('.skill-group:not(.is-selected) > .skill-group-header');
  await expect(selected).toBeVisible();
  for (const other of await others.all()) await expect(other).not.toBeVisible();
  await page.locator('.skills-list-toggle').click();
  await expect(others.first()).toBeVisible();
  const detail = await page.locator('.skill-detail').boundingBox();
  const other = await others.first().boundingBox();
  expect(detail!.y + detail!.height).toBeLessThanOrEqual(other!.y);
  await others.first().locator('.skill-group-toggle').click();
  await expect(page.locator('.skill-detail')).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});
