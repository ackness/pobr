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
export const statValue = (page: Page, label: string | RegExp): Locator => {
  const row = statRow(page, label).locator('.rail-value');
  if (label !== 'Hit DPS' && (!(label instanceof RegExp) || label.source !== '^Hit DPS$')) return row;
  const equivalentHeadline = page.locator('#character-stats:not(:has(.rail-row .rail-label:text-is("Hit DPS"))) .rail-hero')
    .filter({ has: page.locator('.rail-hero-label').filter({ hasText: /^Combined DPS$/ }) }).locator('.rail-hero-value');
  return row.or(equivalentHeadline);
};

/** An import lands on the Overview. Most specs then work on the Items page, so follow the import there. */
export async function openItemsAfterImport(page: Page) {
  await expect(page.locator('#ov-title')).toBeVisible({ timeout: 90_000 });
  await pageLink(page, 'Items').click();
}
