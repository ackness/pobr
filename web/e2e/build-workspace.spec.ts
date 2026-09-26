import { expect, test, type Page } from '@playwright/test';
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
  await page.getByRole('button', { name: action, exact: true }).click();
  await page.getByRole('textbox', { name: 'Name', exact: true }).fill(name);
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
}
async function importCode(page: Page, value: string, destination: 'newBuild' | 'currentStage' = 'newBuild') {
  await page.getByRole('combobox', { name: 'Import into', exact: true }).selectOption(destination);
  if (destination === 'currentStage') page.once('dialog', dialog => dialog.accept());
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(value);
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible();
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await page.getByRole('button', { name: 'Build', exact: true }).click();
}
async function snapshot(page: Page) {
  return page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!));
}

test('independent builds and stages preserve passive routes and notes across reload and sharing', async ({ page }) => {
  await page.goto('/');
  await ready(page);
  await importCode(page, code(30, '1,2', 'Leveling Ice Shot'));
  await editName(page, 'Rename build', 'Ice Shot');
  await editName(page, 'Rename stage', 'Starter');
  await editName(page, 'Copy current stage', 'Endgame');
  await importCode(page, code(90, '3,4', 'Endgame equipment and passive route'), 'currentStage');
  await page.getByRole('combobox', { name: 'Stage', exact: true }).selectOption({ label: 'Starter' });
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('30');
  expect((await snapshot(page)).state.allocatedNodes).toEqual([1, 2]);
  expect((await snapshot(page)).notes).toBe('Leveling Ice Shot');
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await editName(page, 'New build', 'Whirlwind');
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('1');
  await page.getByLabel('Level', { exact: true }).fill('55');
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await page.reload();
  await ready(page);
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('55');
  await page.getByRole('group', { name: 'My builds', exact: true }).getByRole('button', { name: /^Ice Shot/ }).click();
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('30');
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await page.getByRole('combobox', { name: 'Stage', exact: true }).selectOption({ label: 'Endgame' });
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('90');
  expect((await snapshot(page)).state.allocatedNodes).toEqual([3, 4]);
  await expect(page.locator('.topbar-busy')).toHaveCount(0);

  const downloadPromise = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Share this build · all stages', exact: true }).click();
  const download = await downloadPromise;
  const shared = JSON.parse(readFileSync((await download.path())!, 'utf8'));
  expect(shared.build.stages).toHaveLength(2);
  expect(shared.build.stages[0].saved.notes).toBe('Leveling Ice Shot');
  await page.locator('input[type=file]').setInputFiles((await download.path())!);
  await expect(page.locator('.paper-doll')).toBeVisible();
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  const restored = await snapshot(page);
  expect(restored.workspace.builds).toHaveLength(4);
  const imported = restored.workspace.builds[3];
  expect(imported.name).toBe('Ice Shot');
  expect(imported.id).not.toBe(restored.workspace.builds[1].id);
  expect(imported.stages.map((stage: { saved: { notes: string } }) => stage.saved.notes)).toEqual(['Leveling Ice Shot', 'Endgame equipment and passive route']);
  await page.getByRole('button', { name: 'Build', exact: true }).click();
  const backupPromise = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Download backup', exact: true }).click();
  const backup = await backupPromise;
  expect(JSON.parse(readFileSync((await backup.path())!, 'utf8')).workspace.builds).toHaveLength(4);
  await editName(page, 'New build', 'Temporary');
  page.once('dialog', dialog => dialog.accept());
  await page.locator('input[type=file]').setInputFiles((await backup.path())!);
  await expect(page.locator('.paper-doll')).toBeVisible();
  expect((await snapshot(page)).workspace.builds).toHaveLength(4);
});

test('stage switching preserves unapplied item drafts and fits a narrow viewport', async ({ page }) => {
  await page.goto('/');
  await ready(page);
  await editName(page, 'Rename stage', 'Starter');
  await editName(page, 'Copy current stage', 'Maps');
  await page.getByRole('button', { name: 'Items', exact: true }).click();
  await page.locator('.paper-doll').getByRole('button', { name: 'Ring 1', exact: true }).click();
  const editor = page.getByRole('textbox', { name: 'ring1 item text' });
  await editor.fill('Rarity: NORMAL\nSapphire Ring\nImplicits: 0\n+123 to maximum Life');
  await page.getByRole('combobox', { name: 'Stage', exact: true }).selectOption({ label: 'Starter' });
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await page.getByRole('combobox', { name: 'Stage', exact: true }).selectOption({ label: 'Maps' });
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await page.locator('.paper-doll').getByRole('button', { name: 'Ring 1', exact: true }).click();
  await expect(editor).toHaveValue(/123/);
  await page.setViewportSize({ width: 320, height: 780 });
  await page.getByRole('button', { name: 'Manage builds', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'My builds', exact: true })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});

test('a legacy single save migrates without losing character or notes', async ({ page }) => {
  await page.addInitScript(() => {
    if (localStorage.getItem('pobr-build-state')) return;
    localStorage.setItem('pobr-build-state', JSON.stringify({ version: 1, notes: 'Legacy details', state: {
      pobCode: null, character: { level: 74, class_name: 'Ranger', ascendancy_name: 'Deadeye' },
      allocatedNodes: [1, 2], socketGroups: [], items: [], params: { config_inputs: {} },
    } }));
  });
  await page.goto('/');
  await ready(page);
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('74');
  const migrated = await snapshot(page);
  expect(migrated.notes).toBe('Legacy details');
  expect(migrated.workspace.builds[0].stages[0].saved.notes).toBe('Legacy details');
  expect(migrated.state.allocatedNodes).toEqual([1, 2]);
});


test('failed browser persistence stays visible while in-memory stages remain editable', async ({ page }) => {
  await page.addInitScript(() => {
    const original = Storage.prototype.setItem;
    Storage.prototype.setItem = function(key, value) {
      if (key === 'pobr-build-state') throw new DOMException('Full', 'QuotaExceededError');
      original.call(this, key, value);
    };
  });
  await page.goto('/');
  await ready(page);
  await expect(page.locator('.workspace-save-error')).toBeVisible();
  await page.getByLabel('Level', { exact: true }).fill('42');
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await editName(page, 'Copy current stage', 'Maps');
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('42');
  await expect(page.locator('.workspace-save-error')).toBeVisible();
  await expect(page.locator('.workspace-saved')).toHaveCount(0);
});

test('malformed shared files leave the active stage and browser save untouched', async ({ page }) => {
  await page.goto('/');
  await ready(page);
  await importCode(page, code(30, '1,2', 'Keep this build'));
  const before = await snapshot(page);
  const invalid = { version: 1, notes: 'Invalid import', state: { ...before.state,
    flasks: {}, socketGroups: [{ enabled: true, gems: [null] }], params: { config_inputs: {} } } };
  await page.locator('input[type=file]').setInputFiles({ name: 'invalid.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(invalid)) });
  await expect(page.locator('.calc-error')).toBeVisible();
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
  await page.goto('/');
  await ready(page);
  await importCode(page, historical);
  await page.getByLabel('Level', { exact: true }).fill('81');
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await page.reload();
  await ready(page);
  const restored = await snapshot(page);
  expect(restored.state.treeVersion).toBe('0_1');
  expect(restored.state.socketGroups[0].gems[0].stat_set_index).toBe(2);
  await page.getByRole('button', { name: 'Generate share code', exact: true }).click();
  const shared = await page.getByRole('textbox', { name: 'Share Code', exact: true }).inputValue();
  const xml = inflateSync(Buffer.from(shared, 'base64url')).toString('utf8');
  expect(xml).toContain('treeVersion="0_1"');
  expect(xml).toContain('statSetIndex="2"');
  await page.getByRole('button', { name: 'Tree', exact: true }).click();
  await expect(page.getByRole('main').getByText('Interactive tree editing and route planning are unavailable for historical trees.', { exact: false })).toBeVisible();
  await expect(page.locator('svg.tree-svg')).toHaveCount(0);
});

test('planning catalogs stay on the initialized snapshot after a deployment', async ({ page }) => {
  await page.goto('/');
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
  await page.getByRole('button', { name: 'Skills', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Socket Groups', exact: true })).toBeVisible();
  await expect(page.getByPlaceholder('Search an active gem to add a group…')).toBeEnabled();
  await page.getByRole('button', { name: 'Upgrades', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Plan your next upgrade', exact: true })).toBeVisible();
  expect(manifestReads).toBe(0);
  expect(unexpected).toEqual([]);
});

test('an invalid browser workspace is preserved and can be downloaded for recovery', async ({ page }) => {
  const raw = JSON.stringify({ version: 1, state: { character: { class_name: 'Witch' }, allocatedNodes: [], socketGroups: [], items: [], flasks: {} }, notes: 'Recover me' });
  await page.addInitScript(value => localStorage.setItem('pobr-build-state', value), raw);
  await page.goto('/');
  await expect(page.locator('.boot-error')).toContainText('原始数据已保留', { timeout: 90_000 });
  const downloadPromise = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Download original browser save', exact: true }).click();
  const download = await downloadPromise;
  expect(readFileSync((await download.path())!, 'utf8')).toBe(raw);
  expect(await page.evaluate(() => localStorage.getItem('pobr-build-state'))).toBe(raw);
});


test('new imports keep the current build and replacement requires an explicit destination', async ({ page }) => {
  await page.goto('/');
  await ready(page);
  await editName(page, 'Rename build', 'Keep this build');
  await page.getByLabel('Level', { exact: true }).fill('42');
  await page.getByRole('textbox', { name: 'Notes', exact: true }).fill('My original notes');
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  const original = await snapshot(page);
  await expect(page.getByRole('combobox', { name: 'Import into', exact: true })).toHaveValue('newBuild');
  await importCode(page, code(80, '1,2', 'Imported notes'));
  const imported = await snapshot(page);
  expect(imported.workspace.builds).toHaveLength(2);
  expect(imported.workspace.builds[0]).toEqual(original.workspace.builds[0]);
  await expect(page.locator('.workspace-switcher select')).toHaveCount(0);
  await expect(page.locator('.topbar').getByLabel('PoB loadout', { exact: true })).toHaveCount(0);
  await page.getByRole('group', { name: 'My builds', exact: true }).getByRole('button', { name: /^Keep this build/ }).click();
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('42');
  await expect(page.getByRole('textbox', { name: 'Notes', exact: true })).toHaveValue('My original notes');
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await page.getByRole('combobox', { name: 'Import into', exact: true }).selectOption('currentStage');
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code(95, '3,4', 'Replacement'));
  page.once('dialog', dialog => dialog.dismiss());
  await page.locator('.import-submit').click();
  expect((await snapshot(page)).state.character.level).toBe(42);
  page.once('dialog', dialog => dialog.accept());
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible();
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  const replaced = await snapshot(page);
  expect(replaced.workspace.builds).toHaveLength(2);
  expect(replaced.workspace.activeBuild).toBe(original.workspace.activeBuild);
  expect(replaced.state.character.level).toBe(95);
  expect(replaced.notes).toBe('Replacement');
  expect(replaced.workspace.builds[1]).toEqual(imported.workspace.builds[1]);
});
