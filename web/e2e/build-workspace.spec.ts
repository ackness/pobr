import { expect, test, type Page } from '@playwright/test';
import { deflateSync } from 'node:zlib';
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
async function importCode(page: Page, value: string) {
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
  await importCode(page, code(90, '3,4', 'Endgame equipment and passive route'));
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
  await page.getByRole('combobox', { name: 'Build', exact: true }).selectOption({ label: 'Ice Shot' });
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
  expect(restored.workspace.builds).toHaveLength(3);
  const imported = restored.workspace.builds[2];
  expect(imported.name).toBe('Ice Shot');
  expect(imported.id).not.toBe(restored.workspace.builds[0].id);
  expect(imported.stages.map((stage: { saved: { notes: string } }) => stage.saved.notes)).toEqual(['Leveling Ice Shot', 'Endgame equipment and passive route']);
  await page.getByRole('button', { name: 'Build', exact: true }).click();
  const backupPromise = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Download backup', exact: true }).click();
  const backup = await backupPromise;
  expect(JSON.parse(readFileSync((await backup.path())!, 'utf8')).workspace.builds).toHaveLength(3);
  await editName(page, 'New build', 'Temporary');
  page.once('dialog', dialog => dialog.accept());
  await page.locator('input[type=file]').setInputFiles((await backup.path())!);
  await expect(page.locator('.paper-doll')).toBeVisible();
  expect((await snapshot(page)).workspace.builds).toHaveLength(3);
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
  await expect(page.getByRole('heading', { name: 'Builds & stages', exact: true })).toBeVisible();
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
