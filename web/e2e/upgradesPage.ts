import { expect, type Locator, type Page } from '@playwright/test';
import { bdLibraryButton, pageLink, statValue } from './shell';

/** The results panel rows the Items / Upgrades specs read. Labels are anchored: "Life" must not match "Unreserved Life". */
export const lifeValue = (page: Page): Locator => statValue(page, /^Life$/);
export const dpsValue = (page: Page): Locator => statValue(page, /^Total DPS$/);
export const ehpValue = (page: Page): Locator => statValue(page, /^Effective HP$/);

/** The stage that is open, as the top bar's "Current stage" control shows it. */
export const currentStage = (page: Page): Locator => page.getByRole('combobox', { name: 'Current stage' });

/** Open the BD library, copy the open stage under a new name and let the copy become the open stage. */
export async function copyCurrentStage(page: Page, name: string) {
  await bdLibraryButton(page).click();
  await page.getByRole('button', { name: 'Copy current stage', exact: true }).click();
  await page.getByRole('textbox', { name: 'Name', exact: true }).fill(name);
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
}

/** Switch the open stage of the open BD from the top bar. */
export async function openStage(page: Page, name: string) {
  await currentStage(page).click();
  await page.getByRole('option', { name, exact: true }).click();
  await expect(currentStage(page)).toContainText(name);
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
}


/**
 * Follow a page link and wait for the page to be the current one. The shell commits a page change inside a
 * view transition, one frame after the click, so two quick clicks in a row would otherwise race.
 */
export async function goTo(page: Page, name: string, options?: { force?: boolean }) {
  await pageLink(page, name).click(options);
  await expect(pageLink(page, name)).toHaveAttribute('aria-current', 'page');
}

/** The "Inspected skill" selector at the head of the results panel. */
export const inspectedSkill = (page: Page): Locator => page.locator('#character-stats').getByRole('combobox', { name: 'Inspected skill', exact: true });

/**
 * The results panel eases a number to its new value over a fraction of a second, so a single read right after an
 * edit can land mid-way. Read it once it has stopped moving.
 */
export async function settledText(locator: Locator): Promise<string> {
  let last = '';
  await expect.poll(async () => { const now = await locator.innerText(); const same = now === last; last = now; return same; },
    { intervals: [100, 200, 300] }).toBe(true);
  return last;
}
export async function settledNumber(locator: Locator): Promise<number> {
  return Number((await settledText(locator)).replaceAll(',', ''));
}

/** Open the BD library page and wait until it is the current page (see `goTo`). */
export async function openLibrary(page: Page, options?: { force?: boolean }) {
  await bdLibraryButton(page).click(options);
  await expect(bdLibraryButton(page)).toHaveAttribute('aria-current', 'page');
}

/** The options of an open select are rendered in a portal, so they are found on the page, not inside the panel. */
export const optionNamed = (page: Page, name: string | RegExp): Locator =>
  page.getByRole('option', { name, exact: typeof name === 'string' });

/** Open a select and pick the option with this exact label. */
export async function choose(page: Page, trigger: Locator, name: string | RegExp) {
  await trigger.click();
  await optionNamed(page, name).click();
}

const escapeRegExp = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** A rune or soul core in the socket picker. Options also carry their effect text, so the name is matched as a prefix. */
export const augmentOption = (page: Page, name: string): Locator =>
  page.getByRole('option', { name: name ? new RegExp(`^${escapeRegExp(name)}(\\s|$)`) : /^Empty socket/ });

/** Market settings (budget, currency, server, league) sit in a collapsed section of the Upgrades page. */
export async function openMarketSettings(page: Page) {
  const market = page.locator('.trade-market');
  if (!(await market.evaluate(element => (element as HTMLDetailsElement).open))) await market.locator(':scope > summary').click();
  await expect(market).toHaveJSProperty('open', true);
}

/** Goal, capacity and candidate settings of the support optimizer are folded away until asked for. */
export async function openOptimizerSettings(scope: Page | Locator) {
  const settings = scope.locator('.gem-optimizer-settings');
  if (!(await settings.evaluate(element => (element as HTMLDetailsElement).open))) await settings.locator(':scope > summary').click();
  await expect(settings).toHaveJSProperty('open', true);
}

/** The custom modifier blocks sit on their own tab of the Config page. */
export async function openCustomModifiers(page: Page) {
  const tab = page.getByRole('tab', { name: 'Custom modifiers', exact: true });
  await tab.click();
  await expect(tab).toHaveAttribute('aria-selected', 'true');
}

/**
 * For tests that freeze the page clock: a page change runs inside a view transition, and with the clock frozen its
 * overlay never finishes, so it swallows every later click. Ask for reduced motion before the first page change and
 * the shell switches pages at once, which is the same behaviour minus the animation.
 */
export async function reduceMotion(page: Page) {
  await page.emulateMedia({ reducedMotion: 'reduce' });
}
