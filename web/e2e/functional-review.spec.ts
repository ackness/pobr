import { expect, test, type Locator, type Page } from '@playwright/test';
import { openItemsAfterImport } from './shell';
import { answerConfirm } from './confirmDialog';
import { choose, goTo } from './skillsAndCalcs';
import { readFileSync } from 'node:fs';
import { deflateSync } from 'node:zlib';

// The skill picker at the top of the results panel. `goTo` opens the menu sheet where the page links are folded away.
const inspected = (page: Page) => page.getByRole('combobox', { name: 'Inspected skill', exact: true });
const saved = (page: Page) => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state);
const ring = 'Rarity: RARE\nDraft Ring\nSapphire Ring\n+100 to maximum Life';
const monk = readFileSync('../examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt', 'utf8');
const emptyNotes = deflateSync(`<PathOfBuilding2><Build level="40" className="Witch" mainSocketGroup="1"/><Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree><Skills><Skill enabled="true"><Gem skillId="SparkPlayer" gemId="Metadata/Items/Gems/SkillGemSpark" level="5" quality="0" enabled="true"/></Skill><Skill enabled="true"><Gem skillId="FireballPlayer" gemId="Metadata/Items/Gems/SkillGemFireball" level="5" quality="0" enabled="true"/></Skill></Skills><Items/></PathOfBuilding2>`).toString('base64url');
const remainingScroll = (menu: Locator) => menu.evaluate(element => new Promise<number>(resolve => {
  // Read after scroll handlers and their layout updates have settled.
  requestAnimationFrame(() => requestAnimationFrame(() => resolve(element.scrollHeight - element.clientHeight - element.scrollTop)));
}));
async function start(page: Page, code?: string) {
  await page.goto('/build');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  if (code) {
    await page.getByRole('textbox', { name: 'Build code' }).fill(code);
    await page.locator('.import-submit').click();
    await openItemsAfterImport(page);
    await expect(page.locator('.paper-doll')).toBeVisible();
  }
}

test('item drafts survive slot and page switches; cancel discards them and saving affects only the selected slot', async ({ page }) => {
  await start(page);
  await goTo(page, 'Items');
  await page.getByRole('button', { name: 'Ring 1', exact: true }).click();
  await page.getByRole('textbox', { name: 'ring1 item text' }).fill(ring);
  await page.getByRole('button', { name: 'Ring 2', exact: true }).click();
  await page.getByRole('textbox', { name: 'ring2 item text' }).fill(ring.replace('Draft Ring', 'Second Draft'));
  await goTo(page, 'Calcs');
  expect((await saved(page)).items).toEqual([]);
  await goTo(page, 'Items');
  await page.getByRole('button', { name: 'Ring 1', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'ring1 item text' })).toHaveValue(ring);
  await page.getByRole('button', { name: 'Apply changes', exact: true }).click();
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
  await goTo(page, 'Tree');
  const node = page.locator('circle[data-skill-id="7960"]');
  await expect(node).toHaveClass(/node-allocated/);
  await node.dispatchEvent('click');
  const draft = 'Rarity: RARE\nJewel Draft\nSapphire\n10% increased maximum Energy Shield';
  await page.getByRole('textbox', { name: 'Jewel socket', exact: true }).fill(draft);
  await goTo(page, 'Config');
  await goTo(page, 'Tree');
  await expect(page.getByRole('textbox', { name: 'Jewel socket', exact: true })).toHaveValue(draft);
  await page.getByRole('button', { name: 'Apply changes', exact: true }).click();
  await expect.poll(async () => (await saved(page)).jewels.find((j: { socket_node: number }) => j.socket_node === 7960)?.text).toBe(draft);
  await node.dispatchEvent('click');
  await page.getByRole('button', { name: 'Unallocate socket', exact: true }).click();
  await expect(node).not.toHaveClass(/node-allocated/);
  expect((await saved(page)).allocatedNodes).not.toContain(7960);
  expect((await saved(page)).jewels.some((j: { socket_node: number }) => j.socket_node === 7960)).toBe(false);
  await expect(page.locator('.shell-alert')).toHaveCount(0);
});

test('weapon binding and gem menus remain clickable at the bottom of the scroll viewport', async ({ page }) => {
  await start(page, monk);
  await goTo(page, 'Skills');
  await page.locator('.skills-toolbar input').fill('Fireball');
  await page.locator('.gem-picker-popover').getByRole('option').first().click();
  const trigger = page.getByRole('combobox', { name: 'Skill weapon set', exact: true });
  await trigger.evaluate(element => {
    const main = element.closest('main')!;
    main.scrollTop += element.getBoundingClientRect().bottom - main.getBoundingClientRect().bottom + 12;
  });
  await trigger.click();
  // The drop-down's list is not labelled itself; only one is open.
  const menu = page.getByRole('listbox');
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
  const gemMenu = page.locator('.gem-picker-popover');
  const gemRect = await gemMenu.boundingBox();
  expect(gemRect!.y + gemRect!.height).toBeLessThanOrEqual(mainRect!.y + mainRect!.height);
  await gemMenu.getByRole('option').first().click();
  await expect(search).toHaveValue('');
});

test('equipment menus in short viewports can scroll to and select the final position', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 500 });
  await start(page);
  await goTo(page, 'Items');
  await page.getByRole('button', { name: 'Ring 1', exact: true }).click();
  const trigger = page.getByRole('combobox', { name: 'Equipment position', exact: true });
  await trigger.click();
  // The drop-down's list is not labelled itself; the options scroll inside its viewport.
  const list = page.getByRole('listbox');
  const menu = list.locator('[data-radix-select-viewport]');
  // The list is cut to the room the short viewport leaves, so it has to scroll.
  const box = await list.boundingBox();
  expect(box!.y).toBeGreaterThanOrEqual(0);
  expect(box!.y + box!.height).toBeLessThanOrEqual(500);
  expect(await menu.evaluate(element => element.scrollHeight - element.clientHeight)).toBeGreaterThan(0);
  await menu.hover();
  await page.mouse.wheel(0, 1500);
  await expect.poll(() => remainingScroll(menu)).toBeLessThanOrEqual(1);
  await list.getByRole('option', { name: 'Charm 3', exact: true }).click();
  await expect(trigger).toContainText('Charm 3');
});

test('nested gem lists in short viewports can scroll to and select their final result', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 500 });
  await start(page);
  await goTo(page, 'Skills');
  const search = page.locator('.skills-toolbar input');
  await search.click();
  const menu = page.locator('.gem-picker-list');
  // The list is capped at 20rem (320px) and is shorter still when the viewport leaves less room.
  expect(await menu.evaluate(element => element.clientHeight)).toBeLessThan(320);
  const popover = await page.locator('.gem-picker-popover').boundingBox();
  expect(popover!.y + popover!.height).toBeLessThanOrEqual(500);
  const last = menu.getByRole('option').last();
  const name = await last.locator('.gem-primary').innerText();
  await menu.hover();
  await page.mouse.wheel(0, 100_000);
  await expect.poll(() => remainingScroll(menu)).toBeLessThanOrEqual(1);
  await last.click();
  await expect(search).toHaveValue('');
  await expect(page.locator('.skill-group-title')).toContainText(name);
});

test('cascading socket removal clears refunded attribute choices and preserves choices on retained nodes', async ({ page }) => {
  // A connected Warrior path through socket 2491 to attribute node 51561.
  const allocatedNodes = [47175, 38646, 23570, 41031, 54232, 16168, 25374, 53589, 48670, 51299, 35265, 31903, 37258, 28304, 2491, 51561];
  await page.addInitScript(({ allocatedNodes }) => {
    localStorage.setItem('pobr-v2-build-state', JSON.stringify({ version: 1, notes: '', state: {
      pobCode: null,
      character: { level: 90, class_name: 'Warrior', ascendancy_name: '' },
      allocatedNodes,
      attributeChoices: { 23570: 'dex', 51561: 'str' },
      items: [], flasks: [], socketGroups: [], annotations: {}, params: { config_inputs: {} },
      jewels: [{ socket_node: 2491, text: 'Rarity: RARE\nBranch Jewel\nEmerald\n+50 to maximum Life' }],
    } }));
  }, { allocatedNodes });
  await page.goto('/tree');
  const socket = page.locator('circle[data-skill-id="2491"]');
  await expect(socket).toHaveClass(/node-allocated/, { timeout: 90_000 });
  await socket.dispatchEvent('click');
  await page.getByRole('button', { name: 'Unallocate socket', exact: true }).click();
  await expect(socket).not.toHaveClass(/node-allocated/);
  const removed = await saved(page);
  const ascending = (nodes: number[]) => [...nodes].sort((a, b) => a - b);
  expect(ascending(removed.allocatedNodes)).toEqual(ascending(allocatedNodes.filter(node => node !== 2491 && node !== 51561)));
  expect(removed.jewels).toEqual([]);
  expect(removed.attributeChoices).toEqual({ 23570: 'dex' });

  // Reallocating the branch must not silently restore its previous attribute.
  await page.locator('circle[data-skill-id="25312"]').dispatchEvent('click');
  await expect(page.locator('circle[data-skill-id="51561"]')).toHaveClass(/node-allocated/);
  expect((await saved(page)).attributeChoices).toEqual({ 23570: 'dex' });
  await expect(page.locator('.shell-alert')).toHaveCount(0);
});

test('importing a build with no notes and starting a new character clear previous notes', async ({ page }) => {
  await start(page);
  await page.getByRole('textbox', { name: 'Notes', exact: true }).fill('Notes belonging to the previous build');
  await page.getByRole('textbox', { name: 'Build code' }).fill(emptyNotes);
  await page.locator('.import-submit').click();
  await openItemsAfterImport(page);
  await expect(page.locator('.paper-doll')).toBeVisible();
  await page.locator('.shell-builds-button').click();
  await expect(page.getByRole('textbox', { name: 'Notes', exact: true })).toHaveValue('');
  await page.getByRole('textbox', { name: 'Notes', exact: true }).fill('Notes for the imported build');
  await choose(page, page.locator('.bd-character label').filter({ hasText: 'Class' }).getByRole('combobox'), 'Warrior');
  await answerConfirm(page, true);
  await expect(page.getByRole('textbox', { name: 'Notes', exact: true })).toHaveValue('');
});

test('changing the selected skill hides stale attribution until a new calculation completes', async ({ page }) => {
  await start(page, emptyNotes);
  await goTo(page, 'Calcs');
  await page.getByRole('tab', { name: 'Source Attribution', exact: true }).click();
  await page.getByRole('button', { name: 'Run attribution', exact: true }).click();
  await expect(page.locator('.attribution-table')).toBeVisible();
  await choose(page, inspected(page), '2. Fireball');
  await expect(page.locator('.attribution-table')).toHaveCount(0);
  // The skill overview recalculates at the same time and shows its own status.
  await expect(page.getByRole('region', { name: 'Source Attribution', exact: true }).getByRole('status'))
    .toContainText('Run attribution again');
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
  // A disabled button is named by the reason it cannot be used, so find it by its text.
  for (const action of ['Equip', 'Compare']) {
    const button = amulet.getByRole('button').filter({ hasText: new RegExp(`^${action}$`) });
    await expect(button).toBeDisabled();
    await expect(button).toHaveAccessibleName('Select a compatible equipment position first');
  }
});

test('attribution identifies the whole removed group when its selected active gem is not first', async ({ page }) => {
  const code = deflateSync(`<PathOfBuilding2>
    <Build level="40" className="Witch" mainSocketGroup="1"/>
    <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
    <Skills><Skill enabled="true" mainActiveSkill="2">
      <Gem skillId="SparkPlayer" gemId="Metadata/Items/Gems/SkillGemSpark" level="5" quality="0" enabled="true"/>
      <Gem skillId="FireballPlayer" gemId="Metadata/Items/Gems/SkillGemFireball" level="5" quality="0" enabled="true"/>
    </Skill></Skills><Items/></PathOfBuilding2>`).toString('base64url');
  await start(page, code);
  await expect(inspected(page)).toContainText('Fireball');
  const before = await saved(page);
  await goTo(page, 'Calcs');
  await page.getByRole('tab', { name: 'Source Attribution', exact: true }).click();
  await page.getByRole('button', { name: 'Run attribution', exact: true }).click();
  const group = page.locator('.attribution-table tbody tr').filter({ hasText: 'Skill group' });
  await expect(group).toHaveCount(1);
  await expect(group.locator('td').first()).toContainText('Spark');
  await expect(group.locator('td').first()).toContainText('Fireball');
  expect(await saved(page)).toEqual(before);
});
