import { expect, type Locator, type Page } from '@playwright/test';
import { bindT, type Lang } from '../src/lib/i18n';
import { workspaceText } from '../src/lib/workspaceText';
import { bdLibraryButton } from './shell';

/** The page links across the top, in the language the page is shown in. */
export const mainNav = (page: Page, lang: Lang = 'en-US') => page.getByRole('navigation', { name: bindT(lang)('a11y.mainNavigation') });

/**
 * Opens one of the pages by its label. Below the desktop width the links live in the menu drawer
 * instead of the top bar, so a narrow viewport goes through the menu button.
 */
export async function goTo(page: Page, name: string, lang: Lang = 'en-US') {
  const link = mainNav(page, lang).getByRole('link', { name, exact: true });
  if (await link.isVisible()) { await link.click(); return; }
  await page.locator('.shell-menu-button').click();
  await page.getByRole('dialog').getByRole('link', { name, exact: true }).click();
}

/** Opens the BD library page (the Build page) from wherever the app is. */
export async function openBuild(page: Page) {
  await bdLibraryButton(page).click();
  await expect(page.locator('#build-heading')).toBeVisible();
}

/**
 * The calculation has finished and the save indicator reports a healthy local save. This is what the
 * old "Saved" badge plus the absence of the boot and save error banners used to say.
 */
export async function settled(page: Page) {
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await expect(page.locator('.shell-save-state')).not.toHaveClass(/is-error/);
  await expect(page.locator('.shell-boot[role=alert]')).toHaveCount(0);
}

/** Opens a drop-down made by the app's select control and chooses one of its options. */
export async function choose(page: Page, trigger: Locator, option: string | RegExp) {
  await trigger.click();
  await page.getByRole('option', { name: option, exact: typeof option === 'string' }).click();
}

/** Opens a stage of the current BD by its name, from the top bar. */
export async function selectStage(page: Page, name: string) {
  await choose(page, page.getByRole('combobox', { name: 'Current stage', exact: true }), name);
}

/** Picks where the next import goes: a new BD, or the current stage (which asks before it replaces anything). */
export async function importInto(page: Page, destination: 'newBuild' | 'currentStage', lang: Lang = 'en-US') {
  if (!await page.locator('.bd-import').evaluate(el => (el as HTMLDetailsElement).open)) await page.locator('.bd-import > summary').click();
  const choices = page.getByRole('group', { name: workspaceText(lang).importDestination, exact: true }).getByRole('button');
  await choices.nth(destination === 'newBuild' ? 0 : 1).click();
  await expect(choices.nth(destination === 'newBuild' ? 0 : 1)).toHaveAttribute('aria-pressed', 'true');
}

/** The share and backup actions sit in a section that starts closed. */
export async function openExport(page: Page) {
  const section = page.locator('details.bd-export');
  if (!(await section.evaluate(element => (element as HTMLDetailsElement).open))) await section.locator('summary').click();
  await expect(section).toHaveJSProperty('open', true);
}

/** The imported PoB loadouts section of the Build page; it starts closed. */
export async function openLoadouts(page: Page) {
  const section = page.locator('details.bd-details');
  if (!(await section.evaluate(element => (element as HTMLDetailsElement).open))) await section.locator('summary').click();
  await expect(section).toHaveJSProperty('open', true);
}
export const loadoutSelect = (page: Page) => page.getByRole('combobox', { name: 'PoB loadout', exact: true });

/** The results panel, one entry per number: what the old sidebar showed as `dt` / `dd`. */
export async function readRail(page: Page) {
  return page.locator('#character-stats .rail-row').evaluateAll(rows => rows.map(row => ({
    label: row.querySelector('.rail-label')?.textContent, value: row.querySelector('.rail-value')?.textContent,
  })));
}

/** The results panel once its numbers have stopped easing toward their new values. */
export async function displayedRail(page: Page) {
  let last = JSON.stringify(await readRail(page));
  for (let attempt = 0; attempt < 12; attempt++) {
    await page.waitForTimeout(350);
    const next = JSON.stringify(await readRail(page));
    if (next === last) break;
    last = next;
  }
  return JSON.parse(last) as { label: string | null | undefined; value: string | null | undefined }[];
}

/** A number from the results panel once it has stopped easing toward its new value. */
export async function stableText(locator: Locator) {
  const page = locator.page();
  let last = await locator.textContent();
  for (let attempt = 0; attempt < 12; attempt++) {
    await page.waitForTimeout(350);
    const next = await locator.textContent();
    if (next === last) break;
    last = next;
  }
  return last;
}

/** Closes the settings sheet with its own close button (the label follows the language, the slot does not). */
export async function closeSettings(page: Page) {
  await page.locator('.shell-settings [data-slot="sheet-close"]').click();
  await expect(page.locator('.shell-settings')).toHaveCount(0);
}

/** The support search keeps its goal, capacity and candidate settings in a section that starts closed. */
export async function openSupportSettings(page: Page) {
  const section = page.locator('details.gem-optimizer-settings').first();
  if (!(await section.evaluate(element => (element as HTMLDetailsElement).open))) await section.locator(':scope > summary').click();
  await expect(section).toHaveJSProperty('open', true);
}

/**
 * A build that chose no main skill gets the best one pinned a moment after it has settled, as an edit of its
 * own. Results computed before that moment belong to the old state, so wait for it before computing any.
 */
export async function mainSkillPinned(page: Page) {
  await expect.poll(() => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state.params.main_socket_group), { timeout: 60_000 }).toBeDefined();
  await settled(page);
}
