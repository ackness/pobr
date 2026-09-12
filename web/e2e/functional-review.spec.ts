import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { deflateSync } from 'node:zlib';

const nav = (page: Page, name: string) => page.getByRole('navigation', { name: 'Main navigation' }).getByRole('button', { name, exact: true });
const saved = (page: Page) => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!).state);
const ring = 'Rarity: RARE\nDraft Ring\nSapphire Ring\n+100 to maximum Life';
const monk = readFileSync('../examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt', 'utf8');
const emptyNotes = deflateSync(`<PathOfBuilding2><Build level="40" className="Witch" mainSocketGroup="1"/><Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree><Skills><Skill enabled="true"><Gem skillId="SparkPlayer" gemId="Metadata/Items/Gems/SkillGemSpark" level="5" quality="0" enabled="true"/></Skill><Skill enabled="true"><Gem skillId="FireballPlayer" gemId="Metadata/Items/Gems/SkillGemFireball" level="5" quality="0" enabled="true"/></Skill></Skills><Items/></PathOfBuilding2>`).toString('base64url');
async function start(page: Page, code?: string) {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  if (code) {
    await page.getByRole('textbox', { name: 'Build code' }).fill(code);
    await page.locator('.import-submit').click();
    await expect(page.locator('.paper-doll')).toBeVisible();
  }
}

test('item drafts survive slot and page switches; cancel discards them and saving affects only the selected slot', async ({ page }) => {
  await start(page);
  await nav(page, 'Items').click();
  await page.getByRole('button', { name: 'Ring 1', exact: true }).click();
  await page.getByRole('textbox', { name: 'ring1 item text' }).fill(ring);
  await page.getByRole('button', { name: 'Ring 2', exact: true }).click();
  await page.getByRole('textbox', { name: 'ring2 item text' }).fill(ring.replace('Draft Ring', 'Second Draft'));
  await nav(page, 'Calcs').click();
  expect((await saved(page)).items).toEqual([]);
  await nav(page, 'Items').click();
  await expect(page.getByRole('textbox', { name: 'ring1 item text' })).toHaveValue(ring);
  await page.getByRole('button', { name: 'Save & recalculate', exact: true }).click();
  await expect.poll(async () => (await saved(page)).items).toEqual([{ slot: 'ring1', text: ring }]);
  await page.getByRole('button', { name: 'Ring 2', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'ring2 item text' })).toHaveValue(ring.replace('Draft Ring', 'Second Draft'));
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'ring2 item text' })).not.toHaveValue(ring.replace('Draft Ring', 'Second Draft'));
  await page.getByRole('button', { name: 'Ring 1', exact: true }).click();
  await page.getByRole('button', { name: 'Edit', exact: true }).click();
  await page.getByRole('textbox', { name: 'ring1 item text' }).fill('Cancelled change');
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await page.getByRole('button', { name: 'Edit', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'ring1 item text' })).toHaveValue(ring);
  await expect(page.locator('.editor-draft-notice')).toHaveCount(0);
});

test('jewel drafts survive navigation and unallocating removes both the socket point and its jewel', async ({ page }) => {
  await start(page, monk);
  await nav(page, 'Tree').click();
  const node = page.locator('circle[data-skill-id="7960"]');
  await expect(node).toHaveClass(/node-allocated/);
  await node.dispatchEvent('click');
  const draft = 'Rarity: RARE\nJewel Draft\nSapphire\n10% increased maximum Energy Shield';
  await page.getByRole('textbox', { name: 'Jewel socket', exact: true }).fill(draft);
  await nav(page, 'Config').click();
  await nav(page, 'Tree').click();
  await expect(page.getByRole('textbox', { name: 'Jewel socket', exact: true })).toHaveValue(draft);
  await page.getByRole('button', { name: 'Save & recalculate', exact: true }).click();
  await expect.poll(async () => (await saved(page)).jewels.find((j: { socket_node: number }) => j.socket_node === 7960)?.text).toBe(draft);
  await node.dispatchEvent('click');
  await page.getByRole('button', { name: 'Unallocate socket', exact: true }).click();
  await expect(node).not.toHaveClass(/node-allocated/);
  expect((await saved(page)).allocatedNodes).not.toContain(7960);
  expect((await saved(page)).jewels.some((j: { socket_node: number }) => j.socket_node === 7960)).toBe(false);
  await expect(page.locator('.calc-error')).toHaveCount(0);
});

test('weapon binding and gem menus remain clickable at the bottom of the scroll viewport', async ({ page }) => {
  await start(page, monk);
  await nav(page, 'Skills').click();
  await page.locator('.skills-toolbar input').fill('Fireball');
  await page.locator('.gem-picker-panel').getByRole('option').first().click();
  const trigger = page.getByRole('button', { name: 'Skill weapon set', exact: true });
  await trigger.evaluate(element => {
    const main = element.closest('main')!;
    main.scrollTop += element.getBoundingClientRect().bottom - main.getBoundingClientRect().bottom + 12;
  });
  await trigger.click();
  const menu = page.getByRole('listbox', { name: 'Skill weapon set', exact: true });
  const rect = await menu.boundingBox();
  const mainRect = await page.locator('main').boundingBox();
  expect(rect!.y + rect!.height).toBeLessThanOrEqual(mainRect!.y + mainRect!.height);
  await page.getByRole('option', { name: 'Set 2', exact: true }).click();
  await expect(trigger).toContainText('Set 2');
  const search = page.getByRole('combobox', { name: 'Add a support gem…', exact: true });
  await search.fill('Magnified');
  await search.evaluate(element => {
    const main = element.closest('main')!;
    main.scrollTop += element.getBoundingClientRect().bottom - main.getBoundingClientRect().bottom + 12;
  });
  const gemMenu = page.locator('.gem-picker-panel');
  const gemRect = await gemMenu.boundingBox();
  expect(gemRect!.y + gemRect!.height).toBeLessThanOrEqual(mainRect!.y + mainRect!.height);
  await gemMenu.getByRole('option').first().click();
  await expect(search).toHaveValue('');
});

test('importing a build with no notes and starting a new character clear previous notes', async ({ page }) => {
  await start(page);
  await page.getByRole('textbox', { name: 'Notes', exact: true }).fill('Notes belonging to the previous build');
  await page.getByRole('textbox', { name: 'Build code' }).fill(emptyNotes);
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible();
  await nav(page, 'Build').click();
  await expect(page.getByRole('textbox', { name: 'Notes', exact: true })).toHaveValue('');
  await page.getByRole('textbox', { name: 'Notes', exact: true }).fill('Notes for the imported build');
  page.once('dialog', dialog => dialog.accept());
  await page.getByRole('combobox', { name: 'Class', exact: true }).selectOption('Warrior');
  await expect(page.getByRole('textbox', { name: 'Notes', exact: true })).toHaveValue('');
});

test('changing the selected skill hides stale attribution until a new calculation completes', async ({ page }) => {
  await start(page, emptyNotes);
  await nav(page, 'Calcs').click();
  await page.getByRole('button', { name: 'Run attribution', exact: true }).click();
  await expect(page.locator('.attribution-table')).toBeVisible();
  await page.getByRole('combobox', { name: 'Main Skill', exact: true }).selectOption('1');
  await expect(page.locator('.attribution-table')).toHaveCount(0);
  await expect(page.getByRole('status')).toContainText('Run attribution again');
  await page.getByRole('button', { name: 'Run attribution', exact: true }).click();
  await expect(page.locator('.attribution-table')).toBeVisible();
  await expect(page.locator('.attribution-table')).toContainText('Group 2 · Fireball');
});

test('library utilities equip into utility slots and incompatible equipment cannot be applied', async ({ page }) => {
  await start(page, monk);
  await page.getByRole('button', { name: 'Flask 1', exact: true }).click();
  const original = (await saved(page)).flasks.find((item: { slot: string }) => item.slot === 'Flask 1').text;
  await page.locator('.item-detail-header').getByRole('button', { name: 'Save to library', exact: true }).click();
  await page.locator('.item-detail-header').getByRole('button', { name: 'Remove', exact: true }).click();
  const library = page.locator('.items-library');
  await library.getByRole('button', { name: 'Equip', exact: true }).click();
  await expect.poll(async () => (await saved(page)).flasks.find((item: { slot: string }) => item.slot === 'Flask 1')?.text).toBe(original);
  expect((await saved(page)).items.some((item: { slot: string }) => item.slot === 'Flask 1')).toBe(false);
  await library.getByRole('checkbox', { name: /Current slot only/ }).uncheck();
  const amulet = library.locator('.library-row').filter({ hasText: 'Spirit Choker' });
  await expect(amulet.getByRole('button', { name: 'Equip', exact: true })).toBeDisabled();
  await expect(amulet.getByRole('button', { name: 'Compare', exact: true })).toBeDisabled();
});
