import { expect, test, type Page } from '@playwright/test';
import { pageLink, statValue } from './shell';
import { displayedRail, goTo, importInto, openBuild, openSupportSettings, selectStage, settled } from './buildLibrary';
import { answerConfirm } from './confirmDialog';
import { readFileSync } from 'node:fs';
import type { BuildJson, CalculateBuildRequest, CalculateBuildResponse, PassiveNode } from '../src/api/types';
import type { SavedSession } from '../src/hooks/useBuildSession';
import type { BuildWorkspace } from '../src/lib/buildWorkspace';
import { bindT, type Lang } from '../src/lib/i18n';
import { workspaceText } from '../src/lib/workspaceText';
import * as wasmReplay from '../src/wasm/pkg/pobr_wasm.js';

const frostBomb = readFileSync(new URL('../../examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt', import.meta.url), 'utf8').trim();
const legacyBuild = readFileSync(new URL('../../examples/build-file/召唤奥黛丽黑本.build', import.meta.url), 'utf8');
const version = readFileSync(new URL('../../data/CURRENT', import.meta.url), 'utf8').trim();
const passiveNodes: PassiveNode[] = JSON.parse(readFileSync(new URL(`../../data/${version}/base/passive_tree.json`, import.meta.url), 'utf8'));
type Envelope = SavedSession & { workspace: BuildWorkspace };
type Destination = 'newBuild' | 'currentStage';
interface BuildCapture {
  bytes: { workspace: string | null; library: string | null };
  envelope: Envelope;
  request: CalculateBuildRequest;
  result: CalculateBuildResponse;
  displayed: { label: string | null | undefined; value: string | null | undefined }[];
}
const saved = (page: Page): Promise<Envelope> => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!));
const bytes = (page: Page) => page.evaluate(() => ({
  workspace: localStorage.getItem('pobr-v2-build-state'), library: localStorage.getItem('pobr-v2-library'),
}));
const numeric = (text: string) => Number(text.replaceAll(',', ''));

// Named synthetic WeGame source, not a backend/scoring substitute. This fixture
// provides a bounded, improving support search; all decoding/calculation is WASM.
const wegameCaster = {
  format: 'wegame', version: 1, role: { level: 85, class_name: 'Sorceress' },
  equipments: [{ inventoryId: 'Ring', x: 0, baseType: 'Sapphire Ring', name: 'Synthetic JSON Identity Ring', frameType: 2,
    explicitMods: ['40% increased Fire Damage', '+80 to maximum Life'] }],
  talent_tree: { hashes: [54447, 4739, 22419, 18407], quest_stats: [] }, jewel_data: '[]',
  skills: [{ baseType: 'Fireball', support: false,
    properties: [{ type: 5, values: [['16', 0]] }, { type: 6, values: [['+20%', 1]] }] }],
};

let replayPromise: Promise<typeof wasmReplay> | undefined;
async function replayBackend() {
  if (!replayPromise) replayPromise = (async () => {
    await wasmReplay.default({ module_or_path: readFileSync(new URL('../src/wasm/pkg/pobr_wasm_bg.wasm', import.meta.url)) });
    const manifest: { version: string; files: string[] } = JSON.parse(
      readFileSync(new URL('../public/data/manifest.json', import.meta.url), 'utf8'));
    expect(manifest.version).toBe(version);
    for (const file of manifest.files) wasmReplay.stageDataFile(file,
      readFileSync(new URL(`../public/data/${version}/${file}`, import.meta.url), 'utf8'));
    wasmReplay.initStagedData();
    return wasmReplay;
  })();
  return replayPromise;
}

/** An import that chose no main skill gets the best one pinned a moment after it has settled. */
async function pinned(page: Page) {
  await expect.poll(async () => (await saved(page)).state.params.main_socket_group).toBeDefined();
}

async function capture(page: Page): Promise<BuildCapture> {
  await settled(page);
  const envelope = await saved(page);
  const state = envelope.state;
  // Independently project the public persisted calculation contract. Do not use
  // useBuildSession's materializer or patch the production backend.
  const request: CalculateBuildRequest = {
    tree_version: state.treeVersion, character: state.character,
    allocated_nodes: state.allocatedNodes, attribute_choices: state.attributeChoices,
    socket_groups: state.socketGroups.map(group => ({ ...group,
      enabled: group.enabled && (!group.weapon_set || group.weapon_set === (state.weaponSwap?.active ?? 1)) })),
    items: state.items, flasks: state.flasks, jewels: state.jewels,
    main_socket_group: state.params.main_socket_group, enemy_tier: state.params.enemy_tier,
    extra_modifiers: state.params.extra_modifiers, custom_modifier_blocks: state.params.custom_modifier_blocks,
    config_inputs: state.params.config_inputs,
  };
  const result: CalculateBuildResponse = JSON.parse((await replayBackend()).calculateBuildJson(JSON.stringify(request)));
  const displayed = await displayedRail(page);
  return { bytes: await bytes(page), envelope, request: JSON.parse(JSON.stringify(request)), result, displayed };
}

function restoredSnapshot({ bytes, envelope, ...calculation }: BuildCapture) {
  const restored = structuredClone(envelope);
  // Reload may reorder JSON keys and omit the optional no-weapon-swap value.
  // Keep every meaningful saved field, raw config type and actual weapon set.
  const normalize = (state: SavedSession['state']) => {
    if (state.weaponSwap == null) delete state.weaponSwap;
  };
  normalize(restored.state);
  for (const build of restored.workspace.builds) {
    for (const stage of build.stages) normalize(stage.saved.state);
  }
  return { ...calculation, envelope: restored, library: bytes.library };
}

async function expectRestored(page: Page, expected: BuildCapture) {
  expect(restoredSnapshot(await capture(page))).toEqual(restoredSnapshot(expected));
}

async function start(page: Page, lang: Lang = 'en-US') {
  // Only unrelated league discovery is intercepted. Source import, shipped data,
  // full-build calculations and optimizer evaluation are unmodified production.
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.addInitScript(lang => localStorage.setItem('pobr-lang', lang), lang);
  await page.goto('/build');
  await expect(page.getByRole('textbox', { name: bindT(lang)('build.code'), exact: true })).toBeEnabled({ timeout: 90_000 });
  await settled(page);
}

async function importSource(page: Page, source: string, lang: Lang = 'en-US') {
  await page.getByRole('textbox', { name: bindT(lang)('build.code'), exact: true }).fill(source);
  await page.locator('.import-submit').click();
  // An import lands on the Overview; follow it to the Items page and come back to the Build page afterwards.
  await expect(page.locator('#ov-title')).toBeVisible({ timeout: 90_000 });
  await goTo(page, bindT(lang)('tab.items'), lang);
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 90_000 });
  await settled(page);
  await expect(page.getByRole('alert')).toHaveCount(0);
  await openBuild(page);
}

async function editName(page: Page, action: string, name: string, lang: Lang = 'en-US') {
  const text = workspaceText(lang);
  // The import form has a "New build" choice of its own; the library action is the later of the two.
  await page.getByRole('button', { name: action, exact: true }).last().click();
  await page.getByRole('textbox', { name: text.name, exact: true }).fill(name);
  await page.getByRole('button', { name: text.save, exact: true }).click();
  await settled(page);
}

async function frostWorkspace(page: Page, lang: Lang = 'en-US') {
  await start(page, lang);
  await importSource(page, frostBomb, lang);
  const tt = bindT(lang);
  const text = workspaceText(lang);
  await editName(page, text.renameBuild, 'JSON identity Invoker', lang);
  await editName(page, text.renameStage, 'Original Frost Bomb', lang);
  await editName(page, text.copyStage, 'Review imports', lang);
  await page.getByRole('textbox', { name: tt('tab.notes'), exact: true }).fill('Keep the original build, both stages, raw configuration and item library.');
  await settled(page);
  expect((await saved(page)).state.character).toMatchObject({ level: 98, class_name: 'Monk', ascendancy_name: 'Invoker' });
}

async function submitPaste(page: Page, value: string, destination: Destination, lang: Lang = 'en-US', cancel = false) {
  await importInto(page, destination, lang);
  await page.getByRole('textbox', { name: bindT(lang)('build.code'), exact: true }).fill(value);
  await page.locator('.import-submit').click();
  if (destination === 'currentStage') await answerConfirm(page, !cancel);
  await settled(page);
}

async function rejectPaste(page: Page, value: string, destination: Destination, before: BuildCapture, lang: Lang = 'en-US') {
  await submitPaste(page, value, destination, lang);
  expect(await capture(page)).toEqual(before);
  await expect(page.getByRole('alert')).toBeVisible();
  await expect(page.getByRole('textbox', { name: bindT(lang)('build.code'), exact: true })).toHaveValue(value);
  await expect(page.getByLabel(bindT(lang)('build.level'), { exact: true })).toHaveValue(String(before.envelope.state.character.level));
}

for (const lang of ['en-US', 'zh-TW', 'zh-CN'] as const) {
  for (const width of [390, 1440]) {
    test(`${lang} ${width}px: unsupported pasted session rejects New build and confirmed Current stage with visible feedback and exact state`, async ({ page }, testInfo) => {
      test.setTimeout(300_000);
      await page.setViewportSize({ width, height: 780 });
      await frostWorkspace(page, lang);
      const before = await capture(page);
      const unsupported = JSON.stringify(before.envelope);
      // Canceled replacement must never invoke the decoder or create an error.
      await submitPaste(page, unsupported, 'currentStage', lang, true);
      await expect(page.getByRole('alert')).toHaveCount(0);
      expect(await capture(page)).toEqual(before);
      await rejectPaste(page, unsupported, 'newBuild', before, lang);
      await rejectPaste(page, unsupported, 'currentStage', before, lang);
      const tt = bindT(lang);
      await expect(page.getByRole('group', { name: workspaceText(lang).importDestination, exact: true })).toBeVisible();
      await expect(page.getByRole('button', { name: workspaceText(lang).importCurrent, exact: true })).toBeVisible();
      await expect(page.getByRole('button', { name: tt('save.import'), exact: true })).toBeVisible();
      const alert = page.getByRole('alert');
      await alert.scrollIntoViewIfNeeded();
      const bounds = await alert.boundingBox();
      expect(bounds).not.toBeNull();
      expect(bounds!.x).toBeGreaterThanOrEqual(0);
      expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(width);
      expect(bounds!.y).toBeGreaterThanOrEqual(0);
      expect(bounds!.y + bounds!.height).toBeLessThanOrEqual(780);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
      await testInfo.attach('localized-import-rejection', { body: await page.screenshot(), contentType: 'image/png' });
      await goTo(page, tt('tab.calcs'), lang);
      await openBuild(page);
      expect(await capture(page)).toEqual(before);
      await page.reload();
      await expect(page.getByLabel(tt('build.level'), { exact: true })).toHaveValue('98', { timeout: 90_000 });
      await expectRestored(page, before);
    });
  }
}

const invalidSources = [
  ['empty object', {}],
  ['version only', { version: 1 }],
  // A name and an ascendancy alone are a valid Build Planner file, so the object carries a field no source has.
  ['unrelated named object', { name: 'Not a character source', ascendancy: 'Monk3', colour: 'red' }],
  ['missing passives', { items: [], skills: [] }],
  ['missing items', { passives: [], skills: [] }],
  ['missing skills', { passives: [], items: [] }],
  ['wrong passives type', { passives: {}, items: [], skills: [] }],
  ['wrong items type', { passives: [], items: null, skills: [] }],
  ['wrong skills type', { passives: [], items: [], skills: 'Fireball' }],
  ['malformed passive entry', { passives: [{}], items: [], skills: [] }],
  ['malformed item entry', { passives: [], items: [{}], skills: [] }],
  ['malformed skill entry', { passives: [], items: [], skills: [{}] }],
  ['unsupported WeGame version', { ...wegameCaster, version: 2 }],
  ['missing WeGame role', { format: 'wegame', version: 1, equipments: [], skills: [] }],
] as const;

test('unrecognized and malformed legacy JSON never creates an empty build or mutates the complete library', async ({ page }) => {
  test.setTimeout(300_000);
  await frostWorkspace(page);
  const before = await capture(page);
  for (const [name, input] of invalidSources) {
    await test.step(name, async () => rejectPaste(page, JSON.stringify(input), 'newBuild', before));
  }
  await rejectPaste(page, '{}', 'currentStage', before);
});

test('external JSON files reject while malformed PoBR files retain their original session and backup lanes', async ({ page }) => {
  test.setTimeout(300_000);
  await frostWorkspace(page);
  const before = await capture(page);
  for (const input of [{}, { version: 1 }, { passives: [], items: [] }]) {
    await page.locator('input[type=file]').setInputFiles({ name: 'unsupported.build', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(input)) });
    await expect(page.getByRole('alert')).toBeVisible();
    expect(await capture(page)).toEqual(before);
  }
  // Own-property identity, not successful validation, selects the saved-file lane.
  const malformed = [
    { version: 1, notes: 'Saved identity must win over typed legacy collections', passives: [], items: [], skills: [] },
    { version: 1, notes: 'Damaged legacy session' },
    { version: 1, state: null, notes: 'Damaged state' },
    { format: 'pobr-build', version: 1, build: { id: 'bad', stages: [] } },
  ];
  for (const input of malformed) {
    await page.locator('input[type=file]').setInputFiles({ name: 'damaged-save.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(input)) });
    await expect(page.getByRole('alert').last()).toBeVisible();
    expect(await capture(page)).toEqual(before);
  }
  const damagedBackup = JSON.stringify({ ...before.envelope, workspace: null });
  await page.locator('input[type=file]').setInputFiles({ name: 'damaged-workspace.json', mimeType: 'application/json', buffer: Buffer.from(damagedBackup) });
  await answerConfirm(page, false);
  await settled(page);
  expect(await capture(page)).toEqual(before);
  await page.locator('input[type=file]').setInputFiles({ name: 'damaged-workspace.json', mimeType: 'application/json', buffer: Buffer.from(damagedBackup) });
  await answerConfirm(page, true);
  await expect(page.getByRole('alert').last()).toBeVisible();
  expect(await capture(page)).toEqual(before);
  await page.reload();
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('98', { timeout: 90_000 });
  await expectRestored(page, before);
});

for (const destination of ['newBuild', 'currentStage'] as const) {
  test(`${destination}: failed Build import preserves compatible selected-support receipt, settings and exact Apply`, async ({ page }) => {
    test.setTimeout(300_000);
    await start(page);
    await importSource(page, JSON.stringify(wegameCaster));
    await pinned(page);
    await settled(page);
    await editName(page, workspaceText('en-US').copyStage, 'Support review');
    await pinned(page);
    await settled(page);
    await openBuild(page);
    await pageLink(page, 'Skills').click();
    const group = page.locator('.skill-group').first();
    const title = group.locator('.skill-group-title');
    if (await title.getAttribute('aria-expanded') === 'false') await title.click();
    const toggle = group.locator('.gem-optimizer-toggle');
    if (await toggle.getAttribute('aria-expanded') === 'false') await toggle.click();
    await expect(group.locator('.gem-optimizer-body')).toBeVisible();
    await openSupportSettings(page);
    const capacity = page.getByRole('spinbutton', { name: 'Unlocked support sockets', exact: true });
    const lineage = page.getByRole('checkbox', { name: 'Include lineage supports', exact: true });
    await capacity.fill('3');
    await lineage.uncheck();
    await page.getByRole('button', { name: 'Calculate support combinations', exact: true }).click();
    await expect(page.locator('.support-plan').first()).toBeVisible({ timeout: 90_000 });
    await expect(page.getByRole('button', { name: 'Calculate support combinations', exact: true })).toBeEnabled();
    const before = await capture(page);
    const receipt = await page.locator('.support-results').innerText();
    const first = page.locator('.support-plan').first();
    const supports = await first.locator('.support-plan-gems > .opt-chip').evaluateAll(elements => elements.map(element => {
      const statSet = element.getAttribute('data-stat-set-index');
      return { skill_id: element.getAttribute('data-skill-id')!,
        level: Number(element.getAttribute('data-level')), quality: Number(element.getAttribute('data-quality')),
        ...(statSet !== null ? { stat_set_index: Number(statSet) } : {}) };
    }));
    const dpsText = await first.locator('.support-plan-metrics > span').filter({ hasText: /^DPS / }).innerText();
    const preview = numeric(dpsText.match(/^DPS ([\d,.]+)/)![1]);
    const totalDps = statValue(page, /^Total DPS$/).first();
    expect(preview).toBeGreaterThan(numeric(await totalDps.innerText()));
    await openBuild(page);
    await submitPaste(page, '{}', 'currentStage', 'en-US', true);
    expect(await capture(page)).toEqual(before);
    await rejectPaste(page, JSON.stringify(before.envelope), destination, before);
    await pageLink(page, 'Calcs').click();
    await pageLink(page, 'Skills').click();
    await expect.poll(() => page.locator('.support-results').innerText()).toBe(receipt);
    // The settings section folds up when the page is left; the values in it are what must have been kept.
    await openSupportSettings(page);
    await expect(capacity).toHaveValue('3');
    await expect(lineage).not.toBeChecked();
    await expect(first.getByRole('button', { name: 'Apply', exact: true })).toBeEnabled();
    expect(await capture(page)).toEqual(before);
    await first.getByRole('button', { name: 'Apply', exact: true }).click();
    await expect.poll(async () => (await saved(page)).state.socketGroups[0].gems.slice(1)).toEqual(supports);
    await settled(page);
    const after = await capture(page);
    const activeBuild = before.envelope.workspace.activeBuild;
    const activeStage = before.envelope.workspace.builds.find(build => build.id === activeBuild)!.activeStage;
    const expected = structuredClone(before.envelope);
    expected.state.socketGroups[0].gems = [before.envelope.state.socketGroups[0].gems[0], ...supports];
    expected.workspace.builds.find(build => build.id === activeBuild)!.stages.find(stage => stage.id === activeStage)!.saved.state = expected.state;
    expect(after.envelope).toEqual(expected);
    expect(after.bytes.library).toBe(before.bytes.library);
    expect(Math.abs(numeric(await totalDps.innerText()) - preview)).toBeLessThanOrEqual(0.06);
    await expect(page.locator('.support-results')).toHaveCount(0);
    await page.reload();
    await expect(page.locator('.skills-toolbar input')).toBeEnabled({ timeout: 90_000 });
    await expectRestored(page, after);
  });
}

async function checkDecodedImport(page: Page, source: string) {
  const decoded: BuildJson = JSON.parse((await replayBackend()).decodeBuildFileJson(source));
  const state = (await saved(page)).state;
  expect(state.character).toEqual(decoded.character);
  expect(state.allocatedNodes).toEqual(decoded.tree.allocated_nodes);
  expect(state.attributeChoices).toEqual(decoded.tree.attribute_choices);
  expect(state.treeVersion).toEqual(decoded.tree.tree_version);
  expect(state.items).toEqual(decoded.items.equipped);
  expect(state.flasks).toEqual(decoded.items.flasks);
  expect(state.jewels).toEqual(decoded.items.socket_jewels);
  // The app leaves out an absent group count, and pins the best skill of an import that chose none.
  const expectedGroups = decoded.socket_groups.map(({ active_skill_id: _active, group_count, ...group }) => ({
    ...group, ...(group_count != null ? { group_count } : {}),
  }));
  expect(state.socketGroups.map((group, index) =>
    expectedGroups[index]?.main_active_skill === undefined ? { ...group, main_active_skill: undefined } : group)).toEqual(expectedGroups);
  expect(state.params.config_inputs).toEqual(decoded.config_inputs);
  expect(state.pobCode).toBeNull();
}

test('committed legacy .build keeps real character, passives, equipment and gems through file import and reload', async ({ page }) => {
  await frostWorkspace(page);
  const before = await saved(page);
  await page.locator('input[type=file]').setInputFiles({ name: '召唤奥黛丽黑本.build', mimeType: 'application/json', buffer: Buffer.from(legacyBuild) });
  await expect(page.locator('#ov-title')).toBeVisible({ timeout: 90_000 });
  await settled(page);
  await pinned(page);
  await checkDecodedImport(page, legacyBuild);
  const after = await capture(page);
  expect(after.envelope.workspace.builds.slice(0, -1)).toEqual(before.workspace.builds);
  expect(after.envelope.state.character).toEqual({ level: 98, class_name: 'Sorceress', ascendancy_name: 'Disciple of Varashta' });
  const source: { passives: { id: string }[]; items: { inventory_id: string; additional_text: string }[] } = JSON.parse(legacyBuild);
  const sourceSlugs = new Set(source.passives.map(passive => passive.id));
  expect(after.envelope.state.allocatedNodes).toEqual([...new Set(passiveNodes
    .filter(node => sourceSlugs.has(node.id)).map(node => node.skill))].sort((a, b) => a - b));
  expect(after.envelope.state.items.find(item => item.slot === 'helmet')!.text).toContain(source.items.find((item: { inventory_id: string }) => item.inventory_id === 'Helm1').additional_text);
  // Identity of real imported content, not merely collection length/no throw.
  expect(after.envelope.state.socketGroups.flatMap(group => group.gems.map(gem => gem.skill_id))).toContain('SummonSkeletalSnipersPlayer');
  await page.reload();
  await expect(page.locator('#ov-title')).toBeVisible({ timeout: 90_000 });
  await expectRestored(page, after);
  await openBuild(page);
  await importSource(page, legacyBuild);
  await pinned(page);
  await checkDecodedImport(page, legacyBuild);
  const pasted = await capture(page);
  const pastedContent = restoredSnapshot(pasted);
  const fileContent = restoredSnapshot(after);
  expect(pastedContent.envelope.state).toEqual(fileContent.envelope.state);
  expect(pastedContent.envelope.workspace.builds.slice(0, -1)).toEqual(fileContent.envelope.workspace.builds);
  expect(pastedContent.library).toBe(fileContent.library);
  expect(pastedContent.request).toEqual(fileContent.request);
  expect(pastedContent.result).toEqual(fileContent.result);
  expect(pastedContent.displayed).toEqual(fileContent.displayed);
});

test('legacy typed empty collections remain valid without optional name or ascendancy and WeGame JSON remains supported', async ({ page }) => {
  await frostWorkspace(page);
  const before = await saved(page);
  const empty = JSON.stringify({ passives: [], items: [], skills: [] });
  await importSource(page, empty);
  await checkDecodedImport(page, empty);
  const emptyState = await capture(page);
  expect(emptyState.envelope.state.character).toEqual({ level: 1, class_name: '', ascendancy_name: '' });
  expect(emptyState.envelope.state.allocatedNodes).toEqual([]);
  expect(emptyState.envelope.state.items).toEqual([]);
  expect(emptyState.envelope.state.socketGroups).toEqual([]);
  expect(emptyState.envelope.workspace.builds.slice(0, -1)).toEqual(before.workspace.builds);
  await page.reload();
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('1', { timeout: 90_000 });
  await expectRestored(page, emptyState);
  await importSource(page, JSON.stringify(wegameCaster));
  await checkDecodedImport(page, JSON.stringify(wegameCaster));
  const wegame = await capture(page);
  expect(wegame.envelope.state.character).toEqual({ level: 85, class_name: 'Sorceress', ascendancy_name: '' });
  expect(wegame.envelope.state.allocatedNodes).toEqual([...wegameCaster.talent_tree.hashes].sort((a, b) => a - b));
  expect(wegame.envelope.state.socketGroups[0].gems).toEqual([{ skill_id: 'FireballPlayer', level: 16, quality: 20 }]);
  expect(wegame.envelope.state.items[0]).toMatchObject({ slot: 'ring1', text: expect.stringContaining('Synthetic JSON Identity Ring') });
  await page.reload();
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('85', { timeout: 90_000 });
  await expectRestored(page, wegame);
});

test('proper pobr-build file and confirmed workspace backup preserve all stages and raw saved values without granting paste support', async ({ page }) => {
  test.setTimeout(300_000);
  await frostWorkspace(page);
  const original = await saved(page);
  // Named saved-file fixture based on the committed build. These mixed raw
  // values expose coercion at restore/reload boundaries, not new config formulas.
  const raw = structuredClone(original);
  raw.state.params.config_inputs.enemyLevel = '82';
  raw.state.params.config_inputs.overridePowerCharges = 2.5;
  raw.state.params.config_inputs.buffOnslaught = false;
  const rawBuild = raw.workspace.builds.find(build => build.id === raw.workspace.activeBuild)!;
  rawBuild.stages.find(stage => stage.id === rawBuild.activeStage)!.saved.state = raw.state;
  const share = { format: 'pobr-build', version: 1, build: rawBuild };
  const before = await capture(page);
  await rejectPaste(page, JSON.stringify(share), 'newBuild', before);
  await page.locator('input[type=file]').setInputFiles({ name: 'pobr-build-stages.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(share)) });
  await expect(page.locator('#ov-title')).toBeVisible();
  await settled(page);
  const shared = await capture(page);
  expect(shared.envelope.workspace.builds.slice(0, -1)).toEqual(before.envelope.workspace.builds);
  const importedBuild = shared.envelope.workspace.builds.at(-1)!;
  expect(importedBuild.id).not.toBe(rawBuild.id);
  expect(importedBuild.name).toBe(rawBuild.name);
  expect(importedBuild.stages.map(stage => ({ name: stage.name, saved: stage.saved, drafts: stage.drafts })))
    .toEqual(rawBuild.stages.map(stage => ({ name: stage.name, saved: stage.saved, drafts: stage.drafts })));
  expect(shared.envelope.state).toEqual(raw.state);
  expect(shared.bytes.library).toBe(before.bytes.library);
  await page.reload();
  await expect(page.locator('#ov-title')).toBeVisible({ timeout: 90_000 });
  await expectRestored(page, shared);
  await openBuild(page);
  const backup = JSON.stringify(shared.envelope);
  await editName(page, workspaceText('en-US').newBuild, 'Temporary before backup restore');
  const temporary = await capture(page);
  await page.locator('input[type=file]').setInputFiles({ name: 'pobr-workspace.json', mimeType: 'application/json', buffer: Buffer.from(backup) });
  await answerConfirm(page, false);
  await settled(page);
  expect(await capture(page)).toEqual(temporary);
  await page.locator('input[type=file]').setInputFiles({ name: 'pobr-workspace.json', mimeType: 'application/json', buffer: Buffer.from(backup) });
  await answerConfirm(page, true);
  await expect(page.locator('#ov-title')).toBeVisible();
  await expectRestored(page, shared);
  await openBuild(page);
  await selectStage(page, 'Original Frost Bomb');
  await settled(page);
  expect((await saved(page)).state).toEqual(rawBuild.stages.find(stage => stage.name === 'Original Frost Bomb')!.saved.state);
  await selectStage(page, 'Review imports');
  await expectRestored(page, shared);
  await page.reload();
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('98', { timeout: 90_000 });
  await expectRestored(page, shared);
  // The exact version-1 shape rejected by paste remains legitimate as a file.
  const sessionFile: SavedSession = { version: 1, state: structuredClone(raw.state), notes: 'Explicit saved-session replacement' };
  sessionFile.state.character.level = 97;
  await importInto(page, 'currentStage');
  await page.locator('input[type=file]').setInputFiles({ name: 'pobr-session.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(sessionFile)) });
  await answerConfirm(page, true);
  await expect(page.locator('#ov-title')).toBeVisible();
  const replaced = await capture(page);
  const expected = structuredClone(shared.envelope);
  expected.state = sessionFile.state;
  expected.notes = sessionFile.notes;
  const expectedBuild = expected.workspace.builds.find(build => build.id === expected.workspace.activeBuild)!;
  expectedBuild.stages.find(stage => stage.id === expectedBuild.activeStage)!.saved = sessionFile;
  expect(replaced.envelope).toEqual(expected);
  expect(replaced.bytes.library).toBe(shared.bytes.library);
  expect(replaced.request.character?.level).toBe(97);
  await page.reload();
  await expect(page.locator('#ov-title')).toBeVisible({ timeout: 90_000 });
  await expectRestored(page, replaced);
});
