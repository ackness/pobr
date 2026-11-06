import { expect, type Locator, type Page } from '@playwright/test';

/** The page links across the top. The BD library is a button beside them, not a page link. */
export const nav = (page: Page) => page.getByRole('navigation', { name: 'Main navigation' });
export const pageLink = (page: Page, name: string) => nav(page).getByRole('link', { name, exact: true });
export const bdLibraryButton = (page: Page) => page.locator('.shell-builds-button');

/** The shell marks the save indicator while a calculation is running; no mark means the numbers are settled. */
export const busy = (page: Page) => page.locator('.shell-save-state[data-busy]');

/** A result in the left-hand results panel, found by its label ("Life", "Hit DPS", …). */
export const statRow = (page: Page, label: string | RegExp): Locator =>
  page.locator('#character-stats .rail-row').filter({ has: page.locator('.rail-label').filter({ hasText: label }) });
export const statValue = (page: Page, label: string | RegExp): Locator => statRow(page, label).locator('.rail-value');

/** The calculated player's Hit DPS, independent of language and deliberately pinned copies. */
export const playerHitDps = (page: Page): Locator =>
  page.locator('#character-stats .rail-headline > .rail-row[data-stat="TotalDPS"] .rail-value');

/** An import lands on the Overview. Most specs then work on the Items page, so follow the import there. */
export async function openItemsAfterImport(page: Page) {
  await expect(page.locator('#ov-title')).toBeVisible({ timeout: 90_000 });
  await pageLink(page, 'Items').click();
}

/** Open the actions belonging to a build row; defaults to the currently open build. */
export async function openBuildMenu(page: Page, name?: string) {
  const rows = page.locator('.bd-list > li');
  const row = name ? rows.filter({ has: page.locator('.bd-build-select strong', { hasText: name }) })
    : rows.filter({ has: page.locator('.bd-build-select[aria-pressed="true"]') });
  // A menu that opened a dialog unmounts only after the dialog's exit; reopening before then
  // lets that late unmount close the new menu.
  await expect(page.getByRole('menu')).toHaveCount(0);
  await row.locator('[data-slot="dropdown-menu-trigger"]').click();
}

export async function buildAction(page: Page, action: string) {
  await openBuildMenu(page);
  await page.getByRole('menuitem', { name: action, exact: true }).click();
}
