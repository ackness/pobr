import { expect, test, type Page } from '@playwright/test';
import { bdLibraryButton, openItemsAfterImport } from './shell';
import { answerConfirm } from './confirmDialog';
import { choose, goTo } from './skillsAndCalcs';
import { deflateSync } from 'node:zlib';

const saved = (page: Page) => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!));

const importedCode = deflateSync(`<PathOfBuilding2>
  <Build level="40" className="Witch" mainSocketGroup="1"/>
  <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
  <Skills><Skill enabled="true"><Gem skillId="SparkPlayer" gemId="Metadata/Items/Gems/SkillGemSpark" level="5" quality="0" enabled="true"/></Skill></Skills>
  <Items/>
  <Config><Input name="enemyLevel" number="82"/><Input name="buffOnslaught" boolean="true"/></Config>
</PathOfBuilding2>`).toString('base64url');

test.beforeEach(async ({ page }) => {
  await page.goto('/build');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
});

test('failed code and file imports retain the editor and current build; a valid file opens the imported build', async ({ page }) => {
  await page.getByLabel('Level', { exact: true }).fill('40');
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  const before = await saved(page);
  const code = page.getByRole('textbox', { name: 'Build code', exact: true });
  await code.fill('not-a-valid-build');
  await page.locator('.import-submit').click();
  await expect(page.getByRole('alert')).toBeVisible();
  await expect(code).toHaveValue('not-a-valid-build');
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('40');
  expect(await saved(page)).toEqual(before);

  await page.locator('input[type="file"]').setInputFiles({ name: 'invalid.build', mimeType: 'application/json', buffer: Buffer.from('{broken') });
  await expect(page.locator('.import-submit')).toBeEnabled();
  await expect(page.getByRole('alert')).toBeVisible();
  expect(await saved(page)).toEqual(before);

  await page.locator('input[type="file"]').setInputFiles({ name: 'build.txt', mimeType: 'text/plain', buffer: Buffer.from(importedCode) });
  // A successful import lands on the Overview of the imported build.
  await expect(page.locator('#ov-title')).toBeVisible({ timeout: 90_000 });
  await goTo(page, 'Items');
  await expect(page.locator('.paper-doll')).toBeVisible();
  await expect(page.getByRole('alert')).toHaveCount(0);
  expect((await saved(page)).state.character.class_name).toBe('Witch');

  await bdLibraryButton(page).click();
  await page.locator('input[type="file"]').setInputFiles({ name: 'backup.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(before)) });
  // A backup replaces every saved build, so the page asks before restoring it.
  await answerConfirm(page, true);
  await expect(page.locator('#ov-title')).toBeVisible({ timeout: 90_000 });
  await goTo(page, 'Items');
  await expect(page.locator('.paper-doll')).toBeVisible();
  expect(await saved(page)).toEqual(before);
});

test('share codes expire after edits and clipboard failure offers selectable text', async ({ page }) => {
  // Sharing sits under "Share & backup", which starts folded.
  await page.locator('details.bd-export > summary').click();
  const generate = page.getByRole('button', { name: 'Generate share code', exact: true });
  await generate.click();
  const output = page.getByRole('textbox', { name: 'Share Code', exact: true });
  await expect(output).toBeVisible();
  const oldCode = await output.inputValue();
  await page.getByRole('textbox', { name: 'Notes', exact: true }).fill('Latest upgrade plan');
  await expect(output).toHaveCount(0);
  await expect(page.getByRole('main').getByRole('status')).toContainText('Your build has changed');
  await generate.click();
  await expect(output).toBeVisible();
  const currentCode = await output.inputValue();
  expect(currentCode).not.toBe(oldCode);

  await page.evaluate(() => Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    value: { writeText: async () => { throw new DOMException('Denied', 'NotAllowedError'); } },
  }));
  await page.getByRole('button', { name: 'Copy', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('Copy failed');
  const manual = page.getByRole('textbox', { name: 'Text to copy manually' });
  await expect(manual).toHaveValue(currentCode);
  await manual.focus();
  expect(await manual.evaluate((element: HTMLTextAreaElement) => element.selectionEnd - element.selectionStart)).toBe(currentCode.length);
});

test('class changes in both entry points can be cancelled without clearing a build', async ({ page }) => {
  await page.getByLabel('Level', { exact: true }).fill('40');
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await page.reload();
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('40', { timeout: 90_000 });
  const before = await saved(page);
  // Both the Build page and the Tree page offer a class picker; each asks before replacing the build.
  const buildClass = page.locator('.bd-character label').filter({ hasText: 'Class' }).getByRole('combobox');
  await choose(page, buildClass, 'Witch');
  await answerConfirm(page, false);
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('40');
  expect(await saved(page)).toEqual(before);

  await goTo(page, 'Tree');
  const treeClass = page.locator('.tree-asc-picker').filter({ hasText: 'Class' }).getByRole('combobox');
  await choose(page, treeClass, 'Witch');
  await answerConfirm(page, false);
  expect(await saved(page)).toEqual(before);
  await choose(page, treeClass, 'Witch');
  await answerConfirm(page, true);
  // A new Witch starts at level 1; the library row of the open build shows it.
  await bdLibraryButton(page).click();
  await expect(page.locator('.bd-list button[aria-pressed="true"]')).toContainText('Lv1 Witch');
});

test('config drafts commit on Enter and reset restores imported values in the saved request', async ({ page }) => {
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(importedCode);
  await page.locator('.import-submit').click();
  await openItemsAfterImport(page);
  await expect(page.locator('.paper-doll')).toBeVisible();
  await goTo(page, 'Config');
  const search = page.getByRole('searchbox', { name: 'Search config options…' });
  await search.fill('enemyLevel');
  const level = page.locator('#config-enemyLevel');
  await expect(level).toHaveValue('82');
  await expect(page.locator('.config-reset')).toHaveCount(0);
  await level.fill('75');
  expect((await saved(page)).state.params.config_inputs.enemyLevel).toBe(82);
  await level.press('Enter');
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  expect((await saved(page)).state.params.config_inputs.enemyLevel).toBe(75);
  await page.getByRole('button', { name: /Restore imported\/default value/ }).click();
  await expect(level).toHaveValue('82');
  expect((await saved(page)).state.params.config_inputs.enemyLevel).toBe(82);
  await level.fill('');
  await level.press('Tab');
  await expect(level).toHaveValue('82');

  await search.fill('buffOnslaught');
  const checkbox = page.locator('#config-buffOnslaught');
  await expect(checkbox).toBeChecked();
  await page.locator('label[for="config-buffOnslaught"]').click();
  await expect(checkbox).not.toBeChecked();
  await page.getByRole('button', { name: /Restore imported\/default value/ }).click();
  await expect(checkbox).toBeChecked();
  expect((await saved(page)).state.params.config_inputs.buffOnslaught).toBe(true);

  await search.fill('no-such-option-123');
  await expect(page.locator('.search-empty')).toBeVisible();
  // The notice for an empty search offers the full list instead of a "clear search" button.
  await page.locator('.search-empty').getByRole('button', { name: 'Show all options', exact: true }).click();
  await expect(search).toHaveValue('');
  await expect(page.locator('.config-section').first()).toBeVisible();
  await goTo(page, 'Calcs');
  await page.locator('.calcs-search').fill('no-such-stat-123');
  await expect(page.locator('.search-empty')).toBeVisible();
  await page.getByRole('button', { name: 'Clear search', exact: true }).click();
  await expect(page.locator('.calcs-metric-row').first()).toBeVisible();
});

test('selects support keyboard selection, Escape and leaving the control', async ({ page }) => {
  await goTo(page, 'Upgrades');
  // The budget's currency sits with the market settings, which start folded.
  await page.locator('details.trade-market > summary').click();
  const currency = page.getByRole('combobox', { name: 'Budget currency', exact: true });
  await currency.focus();
  await currency.press('ArrowUp');
  const list = page.getByRole('listbox');
  await expect(list).toBeVisible();
  // The highlighted option holds the focus: End moves to the last one, Enter picks it and returns to the control.
  await page.keyboard.press('End');
  await expect(list.getByRole('option').last()).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(list).toHaveCount(0);
  await expect(currency).toContainText('Mirror of Kalandra');
  await expect(currency).toBeFocused();
  await currency.press('Enter');
  await page.keyboard.press('Home');
  await expect(list.getByRole('option').first()).toBeFocused();
  // Escape closes the list without changing the choice and gives the focus back.
  await page.keyboard.press('Escape');
  await expect(list).toHaveCount(0);
  await expect(currency).toContainText('Mirror of Kalandra');
  await expect(currency).toBeFocused();
  await currency.click();
  await page.getByRole('option', { name: 'Divine', exact: true }).click();
  await expect(currency).toContainText('Divine');
});

test('gem filters activate by keyboard without adding a gem; Escape can be reopened', async ({ page }) => {
  await goTo(page, 'Skills');
  const picker = page.getByRole('combobox', { name: /Search an active gem/ });
  await picker.fill('Spark');
  await expect(page.locator('.gem-picker-item').first()).toBeVisible();
  // The filter chips are drawn in the popover, outside the field's tab order, so focus a chip directly.
  const all = page.getByRole('button', { name: 'All', exact: true });
  await all.focus();
  await expect(all).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.locator('.skill-group')).toHaveCount(0);
  await picker.focus();
  await picker.press('Escape');
  await expect(picker).toHaveAttribute('aria-expanded', 'false');
  await picker.press('ArrowDown');
  await expect(picker).toHaveAttribute('aria-expanded', 'true');
  await expect(picker).toHaveAttribute('aria-activedescendant', /.+-0$/);
  await picker.press('Enter');
  await expect(page.locator('.skill-group')).toHaveCount(1);
  await expect(page.locator('.skill-group-name')).toContainText('Spark');
});

test('open dropdowns stay within the content area on narrow and wide screens', async ({ page }) => {
  await goTo(page, 'Upgrades');
  await page.locator('details.trade-market > summary').click();
  for (const width of [320, 1440]) {
    await page.setViewportSize({ width, height: 900 });
    const trigger = page.locator('.trade-market-fields .app-select-trigger').last();
    await trigger.scrollIntoViewIfNeeded();
    await trigger.click();
    const list = page.getByRole('listbox');
    await expect(list).toBeVisible();
    // The list is drawn outside the page's own container, so compare it with the content area's bounds.
    const bounds = await list.evaluate(element => {
      const rect = element.getBoundingClientRect();
      const main = document.querySelector('main')!;
      const content = main.getBoundingClientRect();
      return { left: rect.left - content.left, right: content.right - rect.right, overflow: main.scrollWidth - main.clientWidth };
    });
    expect(bounds.left).toBeGreaterThanOrEqual(0);
    expect(bounds.right).toBeGreaterThanOrEqual(0);
    expect(bounds.overflow).toBeLessThanOrEqual(1);
    await page.keyboard.press('Escape');
    await expect(list).toHaveCount(0);
  }
});

test('saved item notes can be edited with the keyboard', async ({ page }) => {
  await goTo(page, 'Items');
  await page.locator('.paper-doll').getByRole('button', { name: 'Ring 1', exact: true }).click();
  await page.getByRole('button', { name: 'Save & recalculate', exact: true }).click();
  await page.getByRole('button', { name: 'Add note' }).click();
  const note = page.getByRole('textbox', { name: 'Note', exact: true });
  await note.fill('Keep this ring');
  await note.press('Tab');
  const edit = page.getByRole('button', { name: 'Click to edit: Keep this ring' });
  await edit.focus();
  await edit.press('Enter');
  await expect(note).toBeFocused();
  await note.fill('Compare resistances first');
  await note.press('Tab');
  await expect(page.locator('.note-block')).toContainText('Compare resistances first');
});
