import { expect, test, type Page } from '@playwright/test';
import { answerConfirm } from './confirmDialog';
import { pageLink } from './shell';
import { importInto, openBuild, openExport, selectStage, settled } from './buildLibrary';
import { deflateSync, inflateSync } from 'node:zlib';
import { readFileSync } from 'node:fs';

const code = (level: number, nodes: string, notes: string) => deflateSync(`<PathOfBuilding2>
<Build level="${level}" className="Ranger" ascendClassName="Deadeye"/>
<Tree activeSpec="1"><Spec nodes="${nodes}" treeVersion="0_5"/></Tree>
<Skills/><Items/><Notes>${notes}</Notes></PathOfBuilding2>`).toString('base64url');

async function ready(page: Page) {
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
}
async function editName(page: Page, action: string, name: string) {
  // The import form has a "New build" choice of its own; the library action is the later of the two.
  await page.getByRole('button', { name: action, exact: true }).last().click();
  await page.getByRole('textbox', { name: 'Name', exact: true }).fill(name);
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
}
/** Imports on the Build page; the import lands on the Overview, so this returns to the Build page afterwards. */
async function importCode(page: Page, value: string, destination: 'newBuild' | 'currentStage' = 'newBuild') {
  await openBuild(page);
  await importInto(page, destination);
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(value);
  await page.locator('.import-submit').click();
  if (destination === 'currentStage') await answerConfirm(page, true);
  await expect(page.locator('#ov-title')).toBeVisible();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await openBuild(page);
}
async function snapshot(page: Page) {
  return page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!));
}

test('independent builds and stages preserve passive routes and notes across reload and sharing', async ({ page }) => {
  await page.goto('/build');
  await ready(page);
  await importCode(page, code(30, '1,2', 'Leveling Ice Shot'));
  await editName(page, 'Rename build', 'Ice Shot');
  await editName(page, 'Rename stage', 'Starter');
  await editName(page, 'Copy current stage', 'Endgame');
  await importCode(page, code(90, '3,4', 'Endgame equipment and passive route'), 'currentStage');
  await selectStage(page, 'Starter');
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('30');
  expect((await snapshot(page)).state.allocatedNodes).toEqual([1, 2]);
  expect((await snapshot(page)).notes).toBe('Leveling Ice Shot');
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await editName(page, 'New build', 'Whirlwind');
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('1');
  await page.getByLabel('Level', { exact: true }).fill('55');
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await page.reload();
  await ready(page);
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('55');
  await page.getByRole('group', { name: 'My builds', exact: true }).getByRole('button', { name: /^Ice Shot/ }).click();
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('30');
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await selectStage(page, 'Endgame');
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('90');
  expect((await snapshot(page)).state.allocatedNodes).toEqual([3, 4]);
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);

  await openExport(page);
  const downloadPromise = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Share this build · all stages', exact: true }).click();
  const download = await downloadPromise;
  const shared = JSON.parse(readFileSync((await download.path())!, 'utf8'));
  expect(shared.build.stages).toHaveLength(2);
  expect(shared.build.stages[0].saved.notes).toBe('Leveling Ice Shot');
  await page.locator('input[type=file]').setInputFiles((await download.path())!);
  await expect(page.locator('#ov-title')).toBeVisible();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  const restored = await snapshot(page);
  expect(restored.workspace.builds).toHaveLength(4);
  const imported = restored.workspace.builds[3];
  expect(imported.name).toBe('Ice Shot');
  expect(imported.id).not.toBe(restored.workspace.builds[1].id);
  expect(imported.stages.map((stage: { saved: { notes: string } }) => stage.saved.notes)).toEqual(['Leveling Ice Shot', 'Endgame equipment and passive route']);
  await openBuild(page);
  await openExport(page);
  const backupPromise = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Download backup', exact: true }).click();
  const backup = await backupPromise;
  expect(JSON.parse(readFileSync((await backup.path())!, 'utf8')).workspace.builds).toHaveLength(4);
  await editName(page, 'New build', 'Temporary');
  await page.locator('input[type=file]').setInputFiles((await backup.path())!);
  await answerConfirm(page, true);
  await expect(page.locator('#ov-title')).toBeVisible();
  expect((await snapshot(page)).workspace.builds).toHaveLength(4);
});

test('stage switching preserves unapplied item drafts and fits a narrow viewport', async ({ page }) => {
  await page.goto('/build');
  await ready(page);
  await editName(page, 'Rename stage', 'Starter');
  await editName(page, 'Copy current stage', 'Maps');
  await pageLink(page, 'Items').click();
  await page.locator('.paper-doll').getByRole('button', { name: 'Ring 1', exact: true }).click();
  const editor = page.getByRole('textbox', { name: 'ring1 item text' });
  await editor.fill('Rarity: NORMAL\nSapphire Ring\nImplicits: 0\n+123 to maximum Life');
  await selectStage(page, 'Starter');
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await selectStage(page, 'Maps');
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await page.locator('.paper-doll').getByRole('button', { name: 'Ring 1', exact: true }).click();
  await expect(editor).toHaveValue(/123/);
  await page.setViewportSize({ width: 320, height: 780 });
  await page.getByRole('button', { name: 'BDs', exact: true }).click();
  await expect(page.locator('#build-heading')).toBeVisible();
  await expect(page.locator('#bd-library-title')).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});

// The first interface kept one saved session (no workspace) under its own key. Opening the app copies it
// over and wraps it into a build; the same single-session shape is also read from the current key.
for (const [where, key] of [['the first interface key', 'pobr-build-state'], ['the current key', 'pobr-v2-build-state']] as const) {
  test(`a legacy single save under ${where} migrates without losing character or notes`, async ({ page }) => {
    await page.addInitScript(key => {
      if (localStorage.getItem('pobr-v2-storage-initialized') || localStorage.getItem('pobr-v2-build-state')) return;
      localStorage.setItem(key, JSON.stringify({ version: 1, notes: 'Legacy details', state: {
        pobCode: null, character: { level: 74, class_name: 'Ranger', ascendancy_name: 'Deadeye' },
        allocatedNodes: [1, 2], socketGroups: [], items: [], params: { config_inputs: {} },
      } }));
    }, key);
    await page.goto('/build');
    await ready(page);
    await expect(page.getByLabel('Level', { exact: true })).toHaveValue('74');
    const migrated = await snapshot(page);
    expect(migrated.notes).toBe('Legacy details');
    expect(migrated.workspace.builds[0].stages[0].saved.notes).toBe('Legacy details');
    expect(migrated.state.allocatedNodes).toEqual([1, 2]);
    // Whatever the first interface saved stays as it was.
    if (key === 'pobr-build-state') expect(JSON.parse((await page.evaluate(() => localStorage.getItem('pobr-build-state')))!).state.character.level).toBe(74);
  });
}

test('failed browser persistence stays visible while in-memory stages remain editable', async ({ page }) => {
  await page.addInitScript(() => {
    const original = Storage.prototype.setItem;
    Storage.prototype.setItem = function(key, value) {
      if (key === 'pobr-v2-build-state') throw new DOMException('Full', 'QuotaExceededError');
      original.call(this, key, value);
    };
  });
  await page.goto('/build');
  await ready(page);
  const failed = page.locator('.shell-save-state.is-error');
  await expect(failed).toBeVisible();
  await expect(page.locator('.shell-alert')).toContainText('Browser storage is full or unavailable');
  await page.getByLabel('Level', { exact: true }).fill('42');
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await editName(page, 'Copy current stage', 'Maps');
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('42');
  await expect(failed).toBeVisible();
  await expect(page.locator('.shell-save-state:not(.is-error)')).toHaveCount(0);
});

test('malformed shared files leave the active stage and browser save untouched', async ({ page }) => {
  await page.goto('/build');
  await ready(page);
  await importCode(page, code(30, '1,2', 'Keep this build'));
  const before = await snapshot(page);
  const invalid = { version: 1, notes: 'Invalid import', state: { ...before.state,
    flasks: {}, socketGroups: [{ enabled: true, gems: [null] }], params: { config_inputs: {} } } };
  await page.locator('input[type=file]').setInputFiles({ name: 'invalid.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(invalid)) });
  await expect(page.getByRole('alert').first()).toBeVisible();
  expect(await snapshot(page)).toEqual(before);
  await page.reload();
  await ready(page);
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('30');
  expect((await snapshot(page)).notes).toBe('Keep this build');
});

test('imported skill forms and historical tree versions survive editing, reload and export', async ({ page }) => {
  const historical = deflateSync(`<PathOfBuilding2><Build level="80" className="Witch"/>
<Tree activeSpec="1"><Spec nodes="770" treeVersion="0_1"/></Tree>
<Skills><Skill enabled="true"><Gem gemId="Metadata/Items/Gems/SkillGemIceNova" skillId="IceNovaPlayer" level="20" quality="0" statSetIndex="2" enabled="true"/></Skill></Skills><Items/></PathOfBuilding2>`).toString('base64url');
  await page.goto('/build');
  await ready(page);
  await importCode(page, historical);
  await page.getByLabel('Level', { exact: true }).fill('81');
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await page.reload();
  await ready(page);
  const restored = await snapshot(page);
  expect(restored.state.treeVersion).toBe('0_1');
  expect(restored.state.socketGroups[0].gems[0].stat_set_index).toBe(2);
  await openExport(page);
  await page.getByRole('button', { name: 'Generate share code', exact: true }).click();
  const shared = await page.getByRole('textbox', { name: 'Share Code', exact: true }).inputValue();
  const xml = inflateSync(Buffer.from(shared, 'base64url')).toString('utf8');
  expect(xml).toContain('treeVersion="0_1"');
  expect(xml).toContain('statSetIndex="2"');
  await pageLink(page, 'Tree').click();
  await expect(page.getByRole('main').getByText('Interactive tree editing and route planning are unavailable for historical trees.', { exact: false })).toBeVisible();
  await expect(page.locator('svg.tree-svg')).toHaveCount(0);
});

test('planning catalogs stay on the initialized snapshot after a deployment', async ({ page }) => {
  await page.goto('/build');
  await ready(page);
  let manifestReads = 0;
  const unexpected: string[] = [];
  await page.route('**/data/manifest.json', route => {
    manifestReads += 1;
    return route.fulfill({ json: { version: '99.99', files: [] } });
  });
  await page.route('**/data/99.99/**', route => {
    unexpected.push(route.request().url());
    return route.abort();
  });
  await pageLink(page, 'Skills').click();
  await expect(page.getByRole('heading', { name: 'Socket Groups', exact: true })).toBeVisible();
  await expect(page.getByPlaceholder('Search an active gem to add a group…')).toBeEnabled();
  await pageLink(page, 'Upgrades').click();
  await expect(page.getByRole('heading', { name: 'Plan your next upgrade', exact: true })).toBeVisible();
  expect(manifestReads).toBe(0);
  expect(unexpected).toEqual([]);
});

// A save that cannot be read is kept byte for byte and offered for download, whether it sits under the current key
// or was carried over from the first interface's key.
for (const [where, key] of [['the current key', 'pobr-v2-build-state'], ['the first interface key', 'pobr-build-state']] as const) {
  test(`an invalid browser workspace under ${where} is preserved and can be downloaded for recovery`, async ({ page }) => {
    const raw = JSON.stringify({ version: 1, state: { character: { class_name: 'Witch' }, allocatedNodes: [], socketGroups: [], items: [], flasks: {} }, notes: 'Recover me' });
    await page.addInitScript(([key, value]) => {
      if (!localStorage.getItem('pobr-v2-storage-initialized') && !localStorage.getItem('pobr-v2-build-state')) localStorage.setItem(key, value);
    }, [key, raw]);
    await page.goto('/build');
    await expect(page.locator('.shell-boot[role=alert]')).toContainText('Your original save is preserved', { timeout: 90_000 });
    const downloadPromise = page.waitForEvent('download');
    await page.getByRole('button', { name: 'Download original save', exact: true }).click();
    const download = await downloadPromise;
    expect(readFileSync((await download.path())!, 'utf8')).toBe(raw);
    expect(await page.evaluate(key => localStorage.getItem(key), key)).toBe(raw);
  });
}


test('new imports keep the current build and replacement requires an explicit destination', async ({ page }) => {
  await page.goto('/build');
  await ready(page);
  await editName(page, 'Rename build', 'Keep this build');
  await page.getByLabel('Level', { exact: true }).fill('42');
  await page.getByRole('textbox', { name: 'Notes', exact: true }).fill('My original notes');
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  const original = await snapshot(page);
  // A new build is the default destination of an import.
  await expect(page.getByRole('group', { name: 'Import into', exact: true }).getByRole('button').first()).toHaveAttribute('aria-pressed', 'true');
  await importCode(page, code(80, '1,2', 'Imported notes'));
  const imported = await snapshot(page);
  expect(imported.workspace.builds).toHaveLength(2);
  expect(imported.workspace.builds[0]).toEqual(original.workspace.builds[0]);
  // The top bar names the build and stage now open; the loadouts of an imported file live on the Build page only.
  await expect(page.getByRole('combobox', { name: 'Current BD', exact: true })).not.toContainText('Keep this build');
  await expect(page.locator('.shell-topbar').getByLabel('PoB loadout', { exact: true })).toHaveCount(0);
  await page.getByRole('group', { name: 'My builds', exact: true }).getByRole('button', { name: /^Keep this build/ }).click();
  await expect(page.getByRole('combobox', { name: 'Current BD', exact: true })).toContainText('Keep this build');
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('42');
  await expect(page.getByRole('textbox', { name: 'Notes', exact: true })).toHaveValue('My original notes');
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await importInto(page, 'currentStage');
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code(95, '3,4', 'Replacement'));
  await page.locator('.import-submit').click();
  await answerConfirm(page, false);
  expect((await snapshot(page)).state.character.level).toBe(42);
  await page.locator('.import-submit').click();
  await answerConfirm(page, true);
  await expect(page.locator('#ov-title')).toBeVisible();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  const replaced = await snapshot(page);
  expect(replaced.workspace.builds).toHaveLength(2);
  expect(replaced.workspace.activeBuild).toBe(original.workspace.activeBuild);
  expect(replaced.state.character.level).toBe(95);
  expect(replaced.notes).toBe('Replacement');
  expect(replaced.workspace.builds[1]).toEqual(imported.workspace.builds[1]);
});

test('automatic import collection deduplicates raw text without losing equipped copies or distinct rolls', async ({ page }) => {
  const ring = 'Rarity: RARE\nTwin Ring\nSapphire Ring\nImplicits: 0\n+50 to maximum Life';
  const rolledRing = ring.replace('+50', '+51');
  const ringCode = (second: string) => deflateSync(`<PathOfBuilding2>
    <Build level="80" className="Ranger"/>
    <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree><Skills/>
    <Items activeItemSet="1"><Item id="1">${ring}</Item><Item id="2">${second}</Item>
      <ItemSet id="1"><Slot name="Ring 1" itemId="1"/><Slot name="Ring 2" itemId="2"/></ItemSet>
    </Items></PathOfBuilding2>`).toString('base64url');
  const library = () => page.evaluate(() =>
    JSON.parse(localStorage.getItem('pobr-v2-library')!).items as { id: string; text: string }[]);
  await page.goto('/build');
  await ready(page);
  await importCode(page, ringCode(ring));
  expect((await snapshot(page)).state.items).toEqual([
    { slot: 'ring1', text: ring }, { slot: 'ring2', text: ring },
  ]);
  const collected = await library();
  expect(collected.map(item => item.text)).toEqual([ring]);
  await importCode(page, ringCode(ring));
  expect(await library()).toEqual(collected);
  await importCode(page, ringCode(rolledRing));
  expect((await library()).map(item => item.text)).toEqual([ring, rolledRing]);
  expect((await snapshot(page)).state.items).toEqual([
    { slot: 'ring1', text: ring }, { slot: 'ring2', text: rolledRing },
  ]);
  await pageLink(page, 'Items').click();
  for (const slot of ['Ring 1', 'Ring 2']) {
    await page.locator('.paper-doll').getByRole('button', { name: slot, exact: true }).click();
    await expect(page.locator('.items-library').getByRole('button', { name: 'Equip', exact: true })).toHaveCount(2);
    await expect(page.locator('.items-library').getByRole('button', { name: 'Equip', exact: true }).first()).toBeEnabled();
  }
  const beforeReload = await library();
  await page.reload();
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 90_000 });
  expect(await library()).toEqual(beforeReload);
});

test('import recovers legacy library slot eligibility without deleting user-owned duplicates', async ({ page }) => {
  const ring = 'Rarity: RARE\nLegacy Ring\nSapphire Ring\nImplicits: 0\n+50 to maximum Life';
  const entries = [
    { id: 'legacy-ring', kind: 'item', name: 'My legacy ring', text: ring },
    { id: 'manual-copy', kind: 'item', name: 'My comparison copy', text: ring, slot: 'ring1' },
  ];
  await page.addInitScript(items => {
    if (!localStorage.getItem('pobr-v2-library')) {
      localStorage.setItem('pobr-v2-library', JSON.stringify({ items, skillSets: [] }));
    }
  }, entries);
  await page.goto('/build');
  await ready(page);
  await pageLink(page, 'Items').click();
  await page.locator('.paper-doll').getByRole('button', { name: 'Ring 2', exact: true }).click();
  await expect(page.locator('.items-library').getByRole('button', { name: 'Equip', exact: true })).toHaveCount(1);
  await page.locator('.shell-builds-button').click();
  await importCode(page, deflateSync(`<PathOfBuilding2>
    <Build level="80" className="Ranger"/><Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
    <Skills/><Items activeItemSet="1"><Item id="1">${ring}</Item>
    <ItemSet id="1"><Slot name="Ring 2" itemId="1"/></ItemSet></Items></PathOfBuilding2>`).toString('base64url'));
  const expected = [{ ...entries[0], slot: 'ring2' }, entries[1]];
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-library')!).items)).toEqual(expected);
  await pageLink(page, 'Items').click();
  await page.locator('.paper-doll').getByRole('button', { name: 'Ring 1', exact: true }).click();
  const equip = page.locator('.items-library').getByRole('button', { name: 'Equip', exact: true });
  await expect(equip).toHaveCount(2);
  await equip.first().click();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  expect((await snapshot(page)).state.items).toEqual(expect.arrayContaining([
    { slot: 'ring1', text: ring }, { slot: 'ring2', text: ring },
  ]));
  await page.reload();
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 90_000 });
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-library')!).items)).toEqual(expected);
});

test('alternate weapon imports remain usable candidates without changing the inactive pair', async ({ page }) => {
  const main = 'Rarity: RARE\nPrimary Bow\nCrude Bow\nImplicits: 0\n+10 to maximum Life';
  const alternate = main.replace('Primary Bow', 'Alternate Bow').replace('+10', '+20');
  const offhand = 'Rarity: NORMAL\nBroadhead Quiver\nImplicits: 0';
  await page.goto('/build');
  await ready(page);
  await importCode(page, deflateSync(`<PathOfBuilding2>
    <Build level="80" className="Ranger"/><Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
    <Skills/><Items activeItemSet="1">
    <Item id="1">${main}</Item><Item id="2">${alternate}</Item><Item id="3">${offhand}</Item>
    <ItemSet id="1" useSecondWeaponSet="false">
    <Slot name="Weapon 1" itemId="1"/><Slot name="Weapon 2" itemId="3"/>
    <Slot name="Weapon 1 Swap" itemId="2"/></ItemSet></Items></PathOfBuilding2>`).toString('base64url'));
  const imported = (await snapshot(page)).state;
  expect(imported.weaponSwap.alternate_items).toEqual([{ slot: 'weapon1', text: alternate }]);
  await pageLink(page, 'Items').click();
  await page.locator('.paper-doll').getByRole('button', { name: 'Main Hand', exact: true }).click();
  const library = page.locator('.items-library');
  await expect(library.getByRole('button', { name: 'Equip', exact: true })).toHaveCount(2);
  await expect(library).not.toContainText('Broadhead Quiver');
  await library.getByRole('searchbox').fill('Alternate Bow');
  await library.getByRole('button', { name: 'Equip', exact: true }).click();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  const equipped = (await snapshot(page)).state;
  expect(equipped.items).toEqual(expect.arrayContaining([
    { slot: 'weapon1', text: alternate }, { slot: 'weapon2', text: offhand },
  ]));
  expect(equipped.weaponSwap).toEqual(imported.weaponSwap);
  await page.getByRole('group', { name: 'Active weapon set', exact: true }).getByRole('button', { name: 'Set 2', exact: true }).click();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  expect((await snapshot(page)).state.items).toEqual([{ slot: 'weapon1', text: alternate }]);
  await page.reload();
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 90_000 });
  expect((await snapshot(page)).state.weaponSwap.active).toBe(2);
  expect((await snapshot(page)).state.items).toEqual([{ slot: 'weapon1', text: alternate }]);
});

test('identical weapon text retains observed main-hand and offhand library eligibility', async ({ page }) => {
  const weapon = 'Rarity: RARE\nTwin Club\nDriftwood Club\nImplicits: 0\n+10 to maximum Life';
  const other = weapon.replace('Twin Club', 'Other Club').replace('+10', '+20');
  const source = (dual: boolean) => deflateSync(`<PathOfBuilding2>
    <Build level="80" className="Warrior"/><Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
    <Skills/><Items activeItemSet="1"><Item id="1">${weapon}</Item><Item id="2">${other}</Item>
    <ItemSet id="1" useSecondWeaponSet="false"><Slot name="Weapon 1" itemId="1"/>
    ${dual ? '<Slot name="Weapon 2" itemId="2"/><Slot name="Weapon 1 Swap" itemId="1"/><Slot name="Weapon 2 Swap" itemId="1"/>' : ''}
    </ItemSet></Items></PathOfBuilding2>`).toString('base64url');
  await page.goto('/build');
  await ready(page);
  await importCode(page, source(false));
  await pageLink(page, 'Items').click();
  await page.locator('.paper-doll').getByRole('button', { name: 'Off Hand', exact: true }).click();
  const library = page.locator('.items-library');
  await expect(library.getByRole('button', { name: 'Equip', exact: true })).toHaveCount(0);
  await page.locator('.shell-builds-button').click();
  await importCode(page, source(true));
  const baseline = (await snapshot(page)).state;
  await pageLink(page, 'Items').click();
  await page.locator('.paper-doll').getByRole('button', { name: 'Off Hand', exact: true }).click();
  await expect(library.getByRole('button', { name: 'Equip', exact: true })).toHaveCount(2);
  await library.getByRole('searchbox').fill('Twin Club');
  await library.getByRole('button', { name: 'Equip', exact: true }).click();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  const applied = (await snapshot(page)).state;
  expect(applied.items).toEqual([{ slot: 'weapon1', text: weapon }, { slot: 'weapon2', text: weapon }]);
  expect(applied.weaponSwap).toEqual(baseline.weaponSwap);
  const collected = await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-library')!).items);
  expect(collected.map((entry: { slot: string; text: string }) => ({ slot: entry.slot, text: entry.text }))).toEqual([
    { slot: 'weapon1', text: weapon }, { slot: 'weapon2', text: other }, { slot: 'weapon2', text: weapon },
  ]);
  await page.locator('.shell-builds-button').click();
  await importCode(page, source(true));
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-library')!).items)).toEqual(collected);
  await page.reload();
  await ready(page);
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-library')!).items)).toEqual(collected);
});
