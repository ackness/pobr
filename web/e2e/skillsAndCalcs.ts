import { expect, type Locator, type Page } from '@playwright/test';
import { bdLibraryButton, busy, pageLink } from './shell';

/** Import a build code on the Build page and wait until the import has landed on the Overview. */
export async function importBuild(page: Page, code: string) {
  await page.goto('/build');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();
  await expect(page.locator('#ov-title')).toBeVisible({ timeout: 90_000 });
  await expect(busy(page)).toHaveCount(0, { timeout: 90_000 });
}

/**
 * Open a stat on the Calcs page: pick it in the master list, then return the detail pane that
 * holds its breakdown. Waits until the breakdown has loaded (the detail is fetched on demand).
 */
export async function openStat(page: Page, id: string): Promise<Locator> {
  const detail = page.locator('#calcs-detail');
  if (!(await detail.count())) await pageLink(page, 'Calcs').click();
  const row = page.locator(`#breakdown-${id}`);
  await row.click();
  await expect(row).toHaveAttribute('aria-pressed', 'true');
  if ((await row.getAttribute('data-detail-kind')) !== 'none') {
    // The loading, stale and error messages share the class on a <p> or carry a role; the table does not.
    await expect(detail.locator('div.breakdown-detail:not([role])').first()).toBeVisible({ timeout: 90_000 });
  }
  return detail;
}

/** The value shown large at the head of the Calcs detail pane. */
export const detailValue = (detail: Locator) => detail.locator('.calcs-detail-value strong');

/**
 * Open the BD library, unfold "Share & backup", generate the PoB share code of the current stage
 * and return it. The library page is where importing and exporting live.
 */
export async function shareCode(page: Page): Promise<string> {
  await bdLibraryButton(page).click();
  const block = page.locator('details.bd-export');
  await expect(block).toBeVisible();
  if (!(await block.evaluate(element => (element as HTMLDetailsElement).open))) await block.locator('summary').click();
  await page.getByRole('button', { name: 'Generate share code', exact: true }).click();
  const output = page.getByRole('textbox', { name: 'Share Code', exact: true });
  await expect(output).toBeVisible();
  return output.inputValue();
}

/** The headline number at the top of the results panel (Combined DPS). */
export const headlineDps = (page: Page) => page.locator('#character-stats .rail-hero-value');

/**
 * Open a page from the main navigation. Below the shell's wide breakpoint the page links sit in the
 * menu sheet, so open that first when the inline links are not shown.
 */
export async function goTo(page: Page, name: string) {
  // A menu sheet from the previous step may still be sliding out; its links must not be mistaken for the page's own.
  await expect(page.locator('[data-slot="sheet-content"]')).toHaveCount(0);
  const link = pageLink(page, name);
  const menu = page.getByRole('button', { name: 'Open menu', exact: true });
  // Either the inline links or the menu button shows (a closing dialog can hide both for a moment).
  await expect(link.or(menu)).toBeVisible();
  if (!(await link.isVisible())) await menu.click();
  await link.click();
}

/** Import a build code, wait for the Overview, then open the named page (Items by default). */
export async function importAndOpen(page: Page, code: string, pageName = 'Items') {
  await importBuild(page, code);
  await goTo(page, pageName);
}

/** Pick an option in one of the app's drop-down controls (a combobox trigger plus a listbox). */
export async function choose(page: Page, trigger: Locator, option: string | RegExp) {
  await trigger.click();
  await page.getByRole('option', { name: option, ...(typeof option === 'string' ? { exact: true } : {}) }).click();
}

/**
 * A number in the results panel eases to its new value for a moment after a change. Read it once it has
 * stopped moving: two reads a little apart agree. Wait for the change itself to arrive before calling this.
 */
export async function settledNumber(read: () => Promise<number>, timeout = 20_000): Promise<number> {
  const deadline = Date.now() + timeout;
  let previous = await read();
  for (;;) {
    await new Promise(resolve => setTimeout(resolve, 400));
    const current = await read();
    if (current === previous) return current;
    if (Date.now() > deadline) throw new Error(`The number kept changing for ${timeout} ms (last ${previous} then ${current}).`);
    previous = current;
  }
}

/**
 * Close the stat inspector and wait until its sheet has left the page, exit animation included. The
 * sheet closes again at once if it is reopened while that animation is still running.
 */
export async function closeInspector(page: Page) {
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(page.locator('.insp')).toHaveCount(0);
  await expect(page.locator('[data-slot="sheet-overlay"], [data-slot="sheet-content"]')).toHaveCount(0);
}
