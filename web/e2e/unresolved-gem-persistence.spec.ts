import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { deflateSync, inflateSync } from 'node:zlib';
import type { SavedSession } from '../src/hooks/useBuildSession';
import type { BuildWorkspace } from '../src/lib/buildWorkspace';

type Envelope = SavedSession & { workspace: BuildWorkspace };
const cometCode = readFileSync(new URL('../../examples/demo-bd-test/builds/sorceress-stormweaver-comet/code.txt', import.meta.url), 'utf8').trim();
const nav = (page: Page, name: string) => page.getByRole('navigation').getByRole('button', { name, exact: true });
const hitDps = (page: Page) => page.locator('.main-skill-section .stat-row dd').first();

async function snapshot(page: Page): Promise<Envelope> {
  return page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!));
}

function activeBuild(envelope: Envelope) {
  return envelope.workspace.builds.find(build => build.id === envelope.workspace.activeBuild)!;
}

function activeSaved(envelope: Envelope): SavedSession {
  const build = activeBuild(envelope);
  return build.stages.find(stage => stage.id === build.activeStage)!.saved;
}

function assertUnresolvedSlot(saved: SavedSession) {
  expect(saved.state.socketGroups[8].gems[0].skill_id).toBe('FirestormPlayer');
  // The committed source explicitly sets count="1" and both enableGlobal flags.
  expect(saved.state.socketGroups[8].gems[1]).toEqual({
    skill_id: '', level: 0, quality: 0, name_spec: 'Unleash',
    count: 1, enable_global_1: true, enable_global_2: true,
  });
  expect(saved.state.socketGroups[8].gems[2].skill_id).toBe('SupportSpellCascadePlayer');
}

async function idle(page: Page) {
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await expect(page.locator('.workspace-saved')).toBeVisible();
  await expect(page.locator('.boot-error')).toHaveCount(0);
  await expect(page.locator('.workspace-save-error')).toHaveCount(0);
}

async function ready(page: Page) {
  await expect(nav(page, 'Build')).toBeEnabled({ timeout: 90_000 });
  await nav(page, 'Build').click();
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled();
  await idle(page);
}

async function importComet(page: Page) {
  await page.goto('/');
  await ready(page);
  const original = await snapshot(page);
  await page.getByRole('combobox', { name: 'Import into', exact: true }).selectOption('newBuild');
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(cometCode);
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 30_000 });
  await idle(page);
  await nav(page, 'Build').click();
  const imported = await snapshot(page);
  expect(imported.workspace.builds[0]).toEqual(original.workspace.builds[0]);
  expect(imported.state.pobCode).toBe(cometCode);
  expect(imported.state.character).toEqual({ class_name: 'Sorceress', ascendancy_name: 'Stormweaver', level: 98 });
  expect(activeSaved(imported)).toEqual({ version: imported.version, notes: imported.notes, state: imported.state });
  assertUnresolvedSlot(activeSaved(imported));
  return imported;
}

async function editName(page: Page, action: string, name: string) {
  await page.getByRole('button', { name: action, exact: true }).click();
  await page.getByRole('textbox', { name: 'Name', exact: true }).fill(name);
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await idle(page);
}

async function selectStage(page: Page, name: string) {
  await page.getByRole('combobox', { name: 'Stage', exact: true }).selectOption({ label: name });
  await idle(page);
}

async function selectBuild(page: Page, name: string) {
  await page.getByRole('group', { name: 'My builds', exact: true })
    .getByRole('button', { name: new RegExp(`^${name}\\b`) }).click();
  await idle(page);
}

async function downloadJson(page: Page, action: string) {
  const pending = page.waitForEvent('download');
  await page.getByRole('button', { name: action, exact: true }).click();
  const download = await pending;
  const path = (await download.path())!;
  return { path, value: JSON.parse(readFileSync(path, 'utf8')) };
}

// Only actual UI actions write browser storage. The committed PoB code, decoding,
// calculation and reload all use the installed real WASM and synced game data.
test('committed Comet import reloads its entire saved stage and unresolved middle slot', async ({ page }) => {
  const before = await importComet(page);
  const beforeDps = await hitDps(page).innerText();
  expect(Number(beforeDps.replaceAll(',', ''))).toBeGreaterThan(0);
  await page.reload();
  await ready(page);
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('98');
  await expect(page.locator('.character-form select').nth(0)).toHaveValue('Sorceress');
  await expect(page.locator('.character-form select').nth(1)).toHaveValue('Stormweaver');
  await expect(page.locator('.character-form select').nth(0)).toBeEnabled();
  await expect(page.locator('.character-form select').nth(1)).toBeEnabled();
  await expect(page.getByRole('textbox', { name: 'Notes', exact: true })).toHaveValue(before.notes);
  await expect(page.getByRole('button', { name: 'Generate share code', exact: true })).toBeEnabled();
  await expect(page.getByRole('button', { name: 'Copy current stage', exact: true })).toBeEnabled();
  expect(await snapshot(page)).toEqual(before);
  await expect(hitDps(page)).toHaveText(beforeDps);
  assertUnresolvedSlot(activeSaved(await snapshot(page)));
});

test('inactive Comet stages survive multibuild reload, switching and deliberate gem edits', async ({ page }) => {
  await importComet(page);
  await editName(page, 'Rename build', 'Comet');
  await editName(page, 'Rename stage', 'Imported');
  const imported = activeSaved(await snapshot(page));
  await editName(page, 'Copy current stage', 'Edited');
  await page.getByLabel('Level', { exact: true }).fill('99');
  await idle(page);
  await page.getByRole('textbox', { name: 'Notes', exact: true }).fill('Deliberate edits — keep the unresolved Unleash slot');
  await idle(page);
  await nav(page, 'Skills').click();
  const firestorm = page.locator('.skill-group').nth(8);
  await firestorm.locator('.skill-group-title').click();
  const activeGem = firestorm.locator('.skill-gem').nth(0);
  await activeGem.getByRole('spinbutton', { name: 'Level', exact: true }).fill('20');
  await idle(page);
  await activeGem.getByRole('spinbutton', { name: 'Quality', exact: true }).fill('21');
  await idle(page);
  const edited = activeSaved(await snapshot(page));
  const expected = structuredClone(imported);
  expected.state.character.level = 99;
  expected.notes = 'Deliberate edits — keep the unresolved Unleash slot';
  expected.state.socketGroups[8].gems[0].level = 20;
  expected.state.socketGroups[8].gems[0].quality = 21;
  expect(edited).toEqual(expected);
  assertUnresolvedSlot(edited);
  await nav(page, 'Build').click();
  await editName(page, 'New build', 'Separate');
  await page.getByLabel('Level', { exact: true }).fill('42');
  await idle(page);
  const beforeReload = await snapshot(page);
  expect(activeBuild(beforeReload).name).toBe('Separate');
  await page.reload();
  await ready(page);
  expect(await snapshot(page)).toEqual(beforeReload);
  await selectBuild(page, 'Comet');
  expect(activeSaved(await snapshot(page))).toEqual(edited);
  await selectStage(page, 'Imported');
  expect(activeSaved(await snapshot(page))).toEqual(imported);
  await selectStage(page, 'Edited');
  expect(activeSaved(await snapshot(page))).toEqual(edited);
  await selectBuild(page, 'Separate');
  expect(activeSaved(await snapshot(page))).toEqual(activeSaved(beforeReload));
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('42');
});

test('Comet all-stage sharing and workspace backup reimport preserve complete saved states', async ({ page }) => {
  await importComet(page);
  await editName(page, 'Rename build', 'Comet');
  await editName(page, 'Rename stage', 'Imported');
  await editName(page, 'Copy current stage', 'Edited');
  await page.getByLabel('Level', { exact: true }).fill('99');
  await idle(page);
  await page.getByRole('textbox', { name: 'Notes', exact: true }).fill('Shared edited stage');
  await idle(page);
  const beforeShare = await snapshot(page);
  const build = activeBuild(beforeShare);
  const shared = await downloadJson(page, 'Share this build · all stages');
  expect(shared.value.build).toEqual(build);
  await page.locator('input[type=file]').setInputFiles(shared.path);
  await expect(page.locator('.paper-doll')).toBeVisible();
  await idle(page);
  await nav(page, 'Build').click();
  const afterShare = await snapshot(page);
  const reimported = activeBuild(afterShare);
  expect(reimported.id).not.toBe(build.id);
  expect(reimported.stages.map(stage => ({ name: stage.name, saved: stage.saved, drafts: stage.drafts })))
    .toEqual(build.stages.map(stage => ({ name: stage.name, saved: stage.saved, drafts: stage.drafts })));
  expect(afterShare.workspace.builds.slice(0, -1)).toEqual(beforeShare.workspace.builds);
  for (const stage of reimported.stages) assertUnresolvedSlot(stage.saved);
  await selectStage(page, 'Imported');
  expect(activeSaved(await snapshot(page))).toEqual(build.stages[0].saved);
  await selectStage(page, 'Edited');
  expect(activeSaved(await snapshot(page))).toEqual(build.stages[1].saved);
  const beforeBackup = await snapshot(page);
  const backup = await downloadJson(page, 'Download backup');
  expect(backup.value).toEqual(beforeBackup);
  await editName(page, 'New build', 'Temporary');
  page.once('dialog', dialog => dialog.accept());
  await page.locator('input[type=file]').setInputFiles(backup.path);
  await expect(page.locator('.paper-doll')).toBeVisible();
  await idle(page);
  await nav(page, 'Build').click();
  expect(await snapshot(page)).toEqual(beforeBackup);
  await page.reload();
  await ready(page);
  expect(await snapshot(page)).toEqual(beforeBackup);
  assertUnresolvedSlot(activeSaved(await snapshot(page)));
});

test('Comet PoB share-code reimport retains the unresolved slot and source gem metadata', async ({ page }) => {
  const before = await importComet(page);
  const beforeDps = await hitDps(page).innerText();
  await page.getByRole('button', { name: 'Generate share code', exact: true }).click();
  const output = page.getByRole('textbox', { name: 'Share Code', exact: true });
  await expect(output).toBeVisible();
  const sharedCode = await output.inputValue();
  const xml = inflateSync(Buffer.from(sharedCode, 'base64url')).toString('utf8');
  const unresolvedMetadata = await page.evaluate(value => {
    const document = new DOMParser().parseFromString(value, 'application/xml');
    const groups = document.querySelectorAll('Skills > SkillSet > Skill');
    const gem = groups[8]?.querySelectorAll('Gem')[1];
    return gem ? Object.fromEntries(Array.from(gem.attributes, attribute => [attribute.name, attribute.value])) : null;
  }, xml);
  expect(unresolvedMetadata).toMatchObject({
    nameSpec: 'Unleash', level: '0', quality: '0',
    count: '1', enableGlobal1: 'true', enableGlobal2: 'true',
  });
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(sharedCode);
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible();
  await idle(page);
  await nav(page, 'Build').click();
  const after = await snapshot(page);
  assertUnresolvedSlot(activeSaved(after));
  for (const key of ['character', 'treeVersion', 'attributeChoices', 'socketGroups'] as const) {
    expect(after.state[key]).toEqual(before.state[key]);
  }
  expect(new Set(after.state.allocatedNodes)).toEqual(new Set(before.state.allocatedNodes));
  // PoB export may add standard false config defaults; imported values stay exact.
  expect(after.state.params).toMatchObject(before.state.params);
  expect(after.notes).toBe(before.notes);
  expect(after.state.pobCode).toBe(sharedCode);
  expect(after.workspace.builds.slice(0, -1)).toEqual(before.workspace.builds);
  expect(before.state.pobCode).toBe(cometCode);
  await expect(hitDps(page)).toHaveText(beforeDps);
  await page.reload();
  await ready(page);
  expect(await snapshot(page)).toEqual(after);
});

test('unresolved gems without count or global weapon attributes keep them absent through sharing', async ({ page }) => {
  const code = deflateSync(`<PathOfBuilding2><Build className="Sorceress" level="85" mainSocketGroup="1"/>
<Skills activeSkillSet="1"><SkillSet id="1"><Skill enabled="true">
<Gem skillId="FireballPlayer" gemId="Metadata/Items/Gems/SkillGemFireball" level="20" quality="0"/>
<Gem nameSpec="Unleash" level="0" quality="0"/>
</Skill></SkillSet></Skills></PathOfBuilding2>`).toString('base64url');
  await page.goto('/');
  await ready(page);
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible();
  await idle(page);
  const unresolved = { skill_id: '', level: 0, quality: 0, name_spec: 'Unleash' };
  expect((await snapshot(page)).state.socketGroups[0].gems[1]).toEqual(unresolved);
  await nav(page, 'Build').click();
  await page.getByRole('button', { name: 'Generate share code', exact: true }).click();
  const output = page.getByRole('textbox', { name: 'Share Code', exact: true });
  await expect(output).toBeVisible();
  const sharedCode = await output.inputValue();
  const xml = inflateSync(Buffer.from(sharedCode, 'base64url')).toString('utf8');
  const attributes = await page.evaluate(value => {
    const document = new DOMParser().parseFromString(value, 'application/xml');
    const gem = document.querySelector('Skills > SkillSet > Skill > Gem[nameSpec="Unleash"]');
    return gem ? Object.fromEntries(Array.from(gem.attributes, attribute => [attribute.name, attribute.value])) : null;
  }, xml);
  expect(attributes).toMatchObject({ nameSpec: 'Unleash', level: '0', quality: '0' });
  for (const attribute of ['count', 'enableGlobal1', 'enableGlobal2']) {
    expect(attributes).not.toHaveProperty(attribute);
  }
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(sharedCode);
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible();
  await idle(page);
  expect((await snapshot(page)).state.socketGroups[0].gems[1]).toEqual(unresolved);
  await page.reload();
  await ready(page);
  expect((await snapshot(page)).state.socketGroups[0].gems[1]).toEqual(unresolved);
});

test('malformed inactive Comet stage import preserves the current workspace and library', async ({ page }) => {
  await importComet(page);
  await editName(page, 'Copy current stage', 'Valid copy');
  const before = await snapshot(page);
  const library = await page.evaluate(() => localStorage.getItem('pobr-library'));
  const malformed = structuredClone(before);
  activeBuild(malformed).stages[0].saved.state.socketGroups[8].gems[1].quality = -1;
  page.once('dialog', dialog => dialog.accept());
  await page.locator('input[type=file]').setInputFiles({
    name: 'malformed-workspace.json', mimeType: 'application/json',
    buffer: Buffer.from(JSON.stringify(malformed)),
  });
  await expect(page.getByRole('alert')).toContainText('invalid workspace file');
  expect(await snapshot(page)).toEqual(before);
  expect(await page.evaluate(() => localStorage.getItem('pobr-library'))).toBe(library);
  await page.reload();
  await ready(page);
  expect(await snapshot(page)).toEqual(before);
});

test('removing an unresolved slot does not change the selected second active skill', async ({ page }) => {
  // Synthetic group isolates the active ordinal; calculation uses real WASM/data.
  const code = deflateSync(`<PathOfBuilding2><Build className="Sorceress" level="85" mainSocketGroup="1"/>
<Skills activeSkillSet="1"><SkillSet id="1"><Skill enabled="true" mainActiveSkill="2">
<Gem skillId="FireballPlayer" gemId="Metadata/Items/Gems/SkillGemFireball" level="20" quality="0"/>
<Gem nameSpec="Unleash" level="0" quality="0"/>
<Gem skillId="SparkPlayer" gemId="Metadata/Items/Gems/SkillGemSpark" level="20" quality="0"/>
</Skill></SkillSet></Skills></PathOfBuilding2>`).toString('base64url');
  await page.goto('/');
  await ready(page);
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible();
  await idle(page);
  await expect(page.locator('.main-skill-select .app-select-value')).toContainText('Spark');
  const beforeDps = await hitDps(page).innerText();
  await nav(page, 'Skills').click();
  const group = page.locator('.skill-group').first();
  await group.locator('.skill-group-title').click();
  await group.locator('.skill-gem').nth(1).getByTitle('Remove gem', { exact: true }).click();
  await idle(page);
  await expect(page.locator('.main-skill-select .app-select-value')).toContainText('Spark');
  await expect(hitDps(page)).toHaveText(beforeDps);
  expect((await snapshot(page)).state.socketGroups[0].main_active_skill).toBe(2);
});

for (const count of [1, 2]) {
  test(`removing an unresolved row from a ${count}-row unresolved-only group remains restorable`, async ({ page }) => {
    const unknowns = Array.from({ length: count }, (_, i) => `<Gem nameSpec="Unknown ${i + 1}" level="0" quality="0"/>`).join('');
    const xml = `<PathOfBuilding2><Build className="Sorceress" level="85" mainSocketGroup="2"/>
<Skills activeSkillSet="1"><SkillSet id="1">
<Skill enabled="true" mainActiveSkill="1">${unknowns}</Skill>
<Skill enabled="true"><Gem gemId="Metadata/Items/Gems/SkillGemFireball" skillId="FireballPlayer" level="20" quality="0"/></Skill>
</SkillSet></Skills></PathOfBuilding2>`;
    const code = deflateSync(Buffer.from(xml)).toString('base64url');
    await page.goto('/');
    await ready(page);
    await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
    await page.locator('.import-submit').click();
    await expect(page.locator('.paper-doll')).toBeVisible();
    await idle(page);
    const before = await snapshot(page);
    const beforeDps = await hitDps(page).innerText();
    await nav(page, 'Skills').click();
    const group = page.locator('.skill-group').first();
    await group.locator('.skill-group-title').click();
    await group.locator('.skill-gem').first().getByTitle('Remove gem', { exact: true }).click();
    await idle(page);
    const after = await snapshot(page);
    expect(after.state.socketGroups[0].gems).toEqual(before.state.socketGroups[0].gems.slice(1));
    await page.reload();
    await expect(page.locator('.main-skill-select .app-select-value')).toContainText('Fireball');
    await idle(page);
    expect((await snapshot(page)).state.socketGroups[0].main_active_skill).toBe(1);
    expect(await snapshot(page)).toEqual(after);
    await expect(hitDps(page)).toHaveText(beforeDps);
  });
}
