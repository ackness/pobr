import { expect, test, type Browser, type Locator, type Page } from '@playwright/test';

/**
 * The shell's pickers are Radix selects: the trigger is a button with role combobox, and the choices
 * are role=option entries in a portal. Open the trigger and pick the entry by its visible text.
 */
export async function choose(page: Page, trigger: Locator, option: string | RegExp) {
  await trigger.click();
  await page.getByRole('option', { name: option, exact: typeof option === 'string' }).click();
  await expect(page.getByRole('listbox')).toHaveCount(0);
}

/** The comparison pickers label stages "<BD name> / <stage name>". */
export const compareStageA = (page: Page) => page.getByRole('combobox', { name: 'Build / stage A', exact: true });
export const compareStageB = (page: Page) => page.getByRole('combobox', { name: 'Build / stage B', exact: true });
export const compareTab = (page: Page, name: string) => page.getByRole('tab', { name: new RegExp(`^${name}`) });

/** The BD library page: open its Export section (collapsed by default), generate the share code and return it. */
export async function generateShareCode(page: Page): Promise<string> {
  await page.locator('.shell-builds-button').click();
  const exportSection = page.locator('details.bd-export');
  if (!await exportSection.evaluate(element => (element as HTMLDetailsElement).open)) await exportSection.locator(':scope > summary').click();
  await page.getByRole('button', { name: 'Generate share code', exact: true }).click();
  const code = page.getByRole('textbox', { name: 'Share Code', exact: true });
  await expect(code).not.toHaveValue('');
  return code.inputValue();
}

const LANGUAGE_LABEL = { 'en-US': 'English', 'zh-TW': '繁體中文', 'zh-CN': '简体中文' } as const;
/** Switch the interface language through the Settings sheet ("Appearance & language"). */
export async function setLanguage(page: Page, lang: keyof typeof LANGUAGE_LABEL) {
  if (await page.evaluate(() => document.documentElement.lang) === lang) return;
  await page.locator('.shell-settings-button').click();
  await page.locator('#shell-language').click();
  await page.getByRole('option', { name: LANGUAGE_LABEL[lang], exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('lang', lang);
  await page.locator('.shell-settings').getByRole('button', { name: /^(Close|关闭|關閉)$/ }).click();
  await expect(page.locator('.shell-settings')).toHaveCount(0);
}

/**
 * A second, independent browser context for comparing a pointer-driven edit with a keyboard-driven one.
 * `browser.newContext` does not inherit the project's storage state, so pass it on: it carries the
 * language the specs rely on (`pobr-lang=en-US`).
 */
export function newControlContext(browser: Browser, viewport?: { width: number; height: number }) {
  const use = test.info().project.use;
  return browser.newContext({ baseURL: use.baseURL, storageState: use.storageState as never, ...(viewport ? { viewport } : {}) });
}
