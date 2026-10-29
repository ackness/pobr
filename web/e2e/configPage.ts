import { expect, type Locator, type Page } from '@playwright/test';
import { workspaceText } from '../src/lib/workspaceText';
import { answerConfirm } from './confirmDialog';
import { bdLibraryButton } from './shell';

export type Lang = 'en-US' | 'zh-CN' | 'zh-TW';

/** The shell's pickers are Radix selects: a trigger button with role combobox, and role=option entries in a portal. */
export async function choose(page: Page, trigger: Locator, option: string | RegExp) {
  await trigger.click();
  await page.getByRole('option', { name: option, exact: typeof option === 'string' }).click();
  await expect(page.getByRole('listbox')).toHaveCount(0);
}

/** The hidden native <select> that sits beside a picker's trigger and mirrors its value. */
export const nativeSelect = (trigger: Locator): Locator => trigger.locator('xpath=following-sibling::select[1]');

/** The value a picker holds (the option value, not its label). */
export const pickerValue = (trigger: Locator): Promise<string> => nativeSelect(trigger).inputValue();

/** Go to a page by its link text, opening the navigation menu first when the window is too narrow for the top bar's links. */
export async function go(page: Page, name: string) {
  const link = page.locator('nav.shell-tabs').getByRole('link', { name, exact: true });
  const menu = page.locator('.shell-menu-button');
  // Right after a reload the shell is still booting: wait for whichever of the two shows up.
  await expect(async () => { expect(await link.isVisible() || await menu.isVisible()).toBe(true); }).toPass({ timeout: 90_000 });
  if (!(await link.isVisible())) await menu.click();
  // The shell commits a page change a frame after the click, so wait for the address to change before the next step.
  const path = (await link.getAttribute('href'))!;
  await link.click();
  await expect(page).toHaveURL(new RegExp(`${path === '/' ? '/' : path}$`));
}

/** The Hit DPS row of the results panel: the first row under the headline, whatever the interface language. */
export const hitDps = (page: Page): Locator =>
  page.locator('.shell-rail #character-stats .rail-headline > .rail-row .rail-value').first();

/** Numbers settle at once with reduced motion, so a read right after a calculation never catches a tween. */
export const noMotion = { reducedMotion: 'reduce' } as const;

/** Wait until nothing is calculating and no error is shown. */
export async function idle(page: Page) {
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await expect(page.locator('.shell-save-state:not(.is-error)')).toHaveCount(1);
  await expect(page.locator('.shell-boot[role=alert], .shell-alert, .calc-error')).toHaveCount(0);
}

/** The app has booted: the shell is on screen and the first calculation has finished. */
export async function booted(page: Page) {
  await expect(bdLibraryButton(page)).toBeVisible({ timeout: 90_000 });
  await idle(page);
}

export async function openBuilds(page: Page) {
  await bdLibraryButton(page).click();
  await expect(page.locator('#build-heading')).toBeVisible();
}

/** Import a code on the Build page and follow it to the Items page. */
export async function importCode(page: Page, code: string, destination: 'newBuild' | 'currentStage' = 'newBuild', lang: Lang = 'en-US') {
  await openBuilds(page);
  const text = workspaceText(lang);
  const group = page.getByRole('group', { name: text.importDestination, exact: true });
  await group.getByRole('button', { name: destination === 'newBuild' ? text.importNew : new RegExp(`^${text.importCurrent}`) }).click();
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();
  if (destination === 'currentStage') await answerConfirm(page, true);
  await expect(page.locator('#ov-title')).toBeVisible({ timeout: 90_000 });
  await go(page, 'Items');
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 30_000 });
}

/** Generate the PoB share code of the open stage on the Build page. */
export async function shareCode(page: Page) {
  await openBuilds(page);
  const generate = page.getByRole('button', { name: 'Generate share code', exact: true });
  if (!(await generate.isVisible())) await page.locator('details.bd-export > summary').click();
  await generate.click();
  return page.getByRole('textbox', { name: 'Share Code', exact: true }).inputValue();
}

/** The name of the open stage, as the top bar's "Current stage" picker shows it. */
export const stagePicker = (page: Page): Locator => page.getByRole('combobox', { name: 'Current stage', exact: true });
export const currentStageName = async (page: Page) => (await stagePicker(page).innerText()).trim();

/** Switch the open stage from the top bar. */
export async function openStage(page: Page, name: string) {
  await choose(page, stagePicker(page), name);
  await expect(stagePicker(page)).toContainText(name);
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
}

/** Copy the open stage under a new name on the Build page; the copy becomes the open stage. */
export async function copyCurrentStage(page: Page, name: string) {
  await openBuilds(page);
  await page.getByRole('button', { name: 'Copy current stage', exact: true }).click();
  await page.getByRole('textbox', { name: 'Name', exact: true }).fill(name);
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(stagePicker(page)).toContainText(name);
  await idle(page);
}

const LANGUAGE_NAMES: Record<Lang, string> = { 'zh-CN': '简体中文', 'zh-TW': '繁體中文', 'en-US': 'English' };

/** Change the interface language from the Settings sheet and close the sheet again. */
export async function setLanguage(page: Page, lang: Lang) {
  if (await page.locator('html').getAttribute('lang') === lang) return;
  await page.locator('.shell-settings-button').click();
  const sheet = page.getByRole('dialog').filter({ has: page.locator('#shell-language') });
  await choose(page, sheet.locator('#shell-language'), LANGUAGE_NAMES[lang]);
  await expect(page.locator('html')).toHaveAttribute('lang', lang);
  await page.keyboard.press('Escape');
  await expect(sheet).toHaveCount(0);
}

/** The Config page control with this saved key. */
export const control = (page: Page, key: string): Locator => page.locator(`[id=${JSON.stringify(`config-${key}`)}]`);
export const row = (page: Page, key: string): Locator => page.locator('.config-item').filter({ has: control(page, key) });

/** Open the Config page (in the open interface language) and search for a setting. */
export async function openSetting(page: Page, key: string, configName = 'Config') {
  await go(page, configName);
  await page.locator('.config-search').fill(key);
  await expect(control(page, key)).toBeVisible();
  await expect(control(page, key)).toBeEnabled();
}

/** The workspace state the build session keeps in this browser. */
export const savedState = <T = any>(page: Page): Promise<T> => page.evaluate(() =>
  JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state);
