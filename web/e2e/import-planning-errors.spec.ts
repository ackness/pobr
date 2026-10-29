import { expect, test, type Locator, type Page, type TestInfo } from '@playwright/test';
import { pageLink, statValue } from './shell';
import { displayedRail, goTo, importInto, mainSkillPinned, openBuild, openSupportSettings, settled } from './buildLibrary';
import { answerConfirm } from './confirmDialog';
import { readFileSync } from 'node:fs';
import type { AttributeChoice, CalculateBuildRequest, CalculateBuildResponse, PassiveNode } from '../src/api/types';
import { formatApiError } from '../src/api/error';
import type { SavedSession } from '../src/hooks/useBuildSession';
import type { BuildWorkspace } from '../src/lib/buildWorkspace';
import { bindT, type Lang } from '../src/lib/i18n';
import { buildPassiveGraph, shortestAllocationPath } from '../src/lib/passiveGraph';
import { formatStatValue, STAT_SECTIONS } from '../src/lib/statDisplay';
import { workspaceText } from '../src/lib/workspaceText';
import * as wasmReplay from '../src/wasm/pkg/pobr_wasm.js';

const frostBomb = readFileSync(new URL('../../examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt', import.meta.url), 'utf8').trim();
const version = readFileSync(new URL('../../data/CURRENT', import.meta.url), 'utf8').trim();
const nodes: PassiveNode[] = JSON.parse(readFileSync(new URL(`../../data/${version}/base/passive_tree.json`, import.meta.url), 'utf8'));
const catalog = JSON.parse(readFileSync(new URL(`../../data/${version}/overlay/passive_jewels.json`, import.meta.url), 'utf8'));
const starts = new Set<string>(Object.values(catalog.class_starts));
const root = nodes.find(node => node.id === catalog.class_starts.witch)!.skill;
const graph = buildPassiveGraph(nodes.filter(node => !node.ascendancy_id && (!starts.has(node.id) || node.skill === root)));
const path = (node: PassiveNode) => shortestAllocationPath(graph, new Set(), root, node.skill);
const socket = nodes.filter(node => node.kind === 'jewel_socket' && node.id.startsWith('jewel_slot'))
  .sort((a, b) => path(a).length - path(b).length || a.skill - b.skill)[0];
const allocated = path(socket);
const originalChoices = Object.fromEntries(nodes.filter(node => node.name === 'Attribute' && allocated.includes(node.skill))
  .map(node => [String(node.skill), 'int' as const]));
const jewel = 'Rarity: MAGIC\nRuby\nImplicits: 0\n+1000 to maximum Life';
const unmodeledJewel = 'Rarity: MAGIC\nRuby\nImplicits: 0\n+100 to maximum Life\nThis jewel grants an unmodeled power';

type Envelope = SavedSession & { workspace: BuildWorkspace };
type Destination = 'newBuild' | 'currentStage';
type Planner = 'route' | 'joint';
interface Capture {
  bytes: { workspace: string | null; library: string | null };
  envelope: Envelope;
  request: CalculateBuildRequest;
  result: CalculateBuildResponse;
  displayed: { label: string | null | undefined; value: string | null | undefined }[];
}
const labels = {
  'en-US': { title: 'Plan passive upgrades', points: 'Point budget', attribute: 'New travel attributes', strength: 'Strength', run: 'Find upgrade routes', preview: 'Show on tree', apply: 'Apply plan' },
  'zh-TW': { title: '天賦提升規劃', points: '可投入點數', attribute: '新增過路屬性點', strength: '力量', run: '尋找提升路線', preview: '在樹上查看', apply: '套用方案' },
  'zh-CN': { title: '天赋提升规划', points: '可投入点数', attribute: '新增过路属性点', strength: '力量', run: '寻找提升路线', preview: '在树上查看', apply: '应用方案' },
};
const jointLabels = {
  'en-US': { title: 'Jewels and passive routes', text: 'Candidate jewel text', socket: 'Candidate socket', add: 'Add jewel placement', run: 'Find jewel and passive plans' },
  'zh-TW': { title: '珠寶與天賦聯合規劃', text: '候選珠寶文字', socket: '候選珠寶孔', add: '加入珠寶位置', run: '尋找珠寶與天賦方案' },
  'zh-CN': { title: '珠宝与天赋联合规划', text: '候选珠宝文本', socket: '候选珠宝孔', add: '添加珠宝位置', run: '寻找珠宝与天赋方案' },
};
const saved = (page: Page): Promise<Envelope> => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!));
const bytes = (page: Page) => page.evaluate(() => ({
  workspace: localStorage.getItem('pobr-v2-build-state'), library: localStorage.getItem('pobr-v2-library'),
}));
const displayed = displayedRail;
const routeResults = (page: Page) => page.locator('.tree-planner-body > .tree-planner-results');
const joint = (page: Page, lang: Lang = 'en-US') => page.getByRole('region', { name: jointLabels[lang].title });
const results = (page: Page, planner: Planner) => planner === 'route' ? routeResults(page) : joint(page).locator('.jewel-plan-results');
const run = (page: Page, planner: Planner = 'route', lang: Lang = 'en-US') => planner === 'route'
  ? page.getByRole('button', { name: labels[lang].run, exact: true })
  : joint(page, lang).getByRole('button', { name: jointLabels[lang].run, exact: true });
const numeric = (text: string) => Number(text.replaceAll(',', ''));

let replayPromise: Promise<typeof wasmReplay> | undefined;
async function replayBackend() {
  if (!replayPromise) replayPromise = (async () => {
    await wasmReplay.default({ module_or_path: readFileSync(new URL('../src/wasm/pkg/pobr_wasm_bg.wasm', import.meta.url)) });
    const manifest: { version: string; files: string[] } = JSON.parse(readFileSync(new URL('../public/data/manifest.json', import.meta.url), 'utf8'));
    expect(manifest.version).toBe(version);
    for (const file of manifest.files) wasmReplay.stageDataFile(file,
      readFileSync(new URL(`../public/data/${version}/${file}`, import.meta.url), 'utf8'));
    wasmReplay.initStagedData();
    return wasmReplay;
  })();
  return replayPromise;
}

function requestOf(state: SavedSession['state']): CalculateBuildRequest {
  // Independent projection of the persisted public contract, not the production
  // materializer; never replace the application's backend or scoring responses.
  return JSON.parse(JSON.stringify({
    tree_version: state.treeVersion, character: state.character,
    allocated_nodes: state.allocatedNodes, attribute_choices: state.attributeChoices,
    socket_groups: state.socketGroups.map(group => ({ ...group,
      enabled: group.enabled && (!group.weapon_set || group.weapon_set === (state.weaponSwap?.active ?? 1)) })),
    items: state.items, flasks: state.flasks, jewels: state.jewels,
    main_socket_group: state.params.main_socket_group, enemy_tier: state.params.enemy_tier,
    extra_modifiers: state.params.extra_modifiers, custom_modifier_blocks: state.params.custom_modifier_blocks,
    config_inputs: state.params.config_inputs,
  }));
}

async function calculate(request: CalculateBuildRequest): Promise<CalculateBuildResponse> {
  return JSON.parse((await replayBackend()).calculateBuildJson(JSON.stringify(request)));
}

async function capture(page: Page): Promise<Capture> {
  await settled(page);
  const envelope = await saved(page);
  const request = requestOf(envelope.state);
  return { bytes: await bytes(page), envelope, request, result: await calculate(request), displayed: await displayed(page) };
}

async function start(page: Page, lang: Lang = 'en-US') {
  // Only unrelated league discovery is intercepted; all data and calculations
  // come from the shipped production app and its real WASM backend.
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.addInitScript(lang => localStorage.setItem('pobr-lang', lang), lang);
  await page.goto('/build');
  await expect(page.getByRole('textbox', { name: bindT(lang)('build.code'), exact: true })).toBeEnabled({ timeout: 90_000 });
  await settled(page);
}

async function copyStage(page: Page, lang: Lang = 'en-US') {
  const text = workspaceText(lang);
  await page.getByRole('button', { name: text.copyStage, exact: true }).click();
  await page.getByRole('textbox', { name: text.name, exact: true }).fill('Import error planning review');
  await page.getByRole('button', { name: text.save, exact: true }).click();
  await page.getByRole('textbox', { name: bindT(lang)('tab.notes'), exact: true }).fill('Keep all other stages, source text, raw config values and the item library.');
  await settled(page);
}

async function frostWorkspace(page: Page, lang: Lang) {
  await start(page, lang);
  await page.getByRole('textbox', { name: bindT(lang)('build.code'), exact: true }).fill(frostBomb);
  await page.locator('.import-submit').click();
  // An import lands on the Overview; follow it to the Items page and come back to the Build page afterwards.
  await expect(page.locator('#ov-title')).toBeVisible({ timeout: 90_000 });
  await goTo(page, bindT(lang)('tab.items'), lang);
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 90_000 });
  await settled(page);
  await openBuild(page);
  await copyStage(page, lang);
  expect((await saved(page)).state.character).toMatchObject({ level: 98, class_name: 'Monk', ascendancy_name: 'Invoker' });
}

async function syntheticWitch(page: Page, lang: Lang = 'en-US') {
  // Named saved-session fixture for bounded improving route/jewel/support plans.
  // Original Intelligence choices and new Strength travel expose exact Apply.
  const fixture: SavedSession = { version: 1, notes: 'Synthetic import planning Witch', state: {
    pobCode: null, character: { level: 90, class_name: 'Witch', ascendancy_name: '' },
    allocatedNodes: allocated, attributeChoices: originalChoices,
    socketGroups: [{ enabled: true, gems: [{ skill_id: 'FireballPlayer', level: 16, quality: 0 }] }],
    items: [{ slot: 'ring1', text: 'Rarity: RARE\nSynthetic Import Planning Ring\nSapphire Ring\nImplicits: 0\n40% increased Fire Damage\n+80 to maximum Life' }],
    flasks: [], jewels: [{ socket_node: socket.skill, text: 'Rarity: MAGIC\nRuby\nImplicits: 0\n+1 to maximum Life' }],
    annotations: { 'skill:0': 'Keep source metadata' },
    params: { config_inputs: { enemyLevel: '82', overridePowerCharges: 2.5, buffOnslaught: false } },
  } };
  await page.addInitScript(fixture => localStorage.setItem('pobr-v2-build-state', JSON.stringify(fixture)), fixture);
  await start(page, lang);
  await copyStage(page, lang);
  await mainSkillPinned(page);
}

async function select(page: Page, label: string, option: string) {
  await page.getByRole('combobox', { name: label, exact: true }).click();
  await page.getByRole('option', { name: option, exact: true }).click();
}

async function openTree(page: Page, lang: Lang = 'en-US') {
  await goTo(page, bindT(lang)('tab.tree'), lang);
  await expect(page.locator('.tree-asc-picker select').first()).toBeEnabled({ timeout: 90_000 });
  const toggle = page.getByRole('button', { name: labels[lang].title, exact: true });
  if (await toggle.getAttribute('aria-expanded') === 'false') await toggle.click();
  await expect(page.locator('.tree-planner-body')).toBeVisible();
}

async function configure(page: Page, lang: Lang = 'en-US') {
  await openTree(page, lang);
  await select(page, bindT(lang)('opt.objective'), bindT(lang)('opt.objLife'));
  await page.getByRole('spinbutton', { name: labels[lang].points, exact: true }).fill('3');
  await select(page, labels[lang].attribute, labels[lang].strength);
  await expect(run(page, 'route', lang)).toBeEnabled();
}

async function paste(page: Page, source: string, destination: Destination, lang: Lang = 'en-US', cancel = false) {
  await openBuild(page);
  await importInto(page, destination, lang);
  await page.getByRole('textbox', { name: bindT(lang)('build.code'), exact: true }).fill(source);
  await page.locator('.import-submit').click();
  if (destination === 'currentStage') await answerConfirm(page, !cancel);
  await settled(page);
}

async function reject(page: Page, before: Capture, destination: Destination, lang: Lang = 'en-US') {
  const source = JSON.stringify(before.envelope);
  await paste(page, source, destination, lang);
  await expect(page.getByRole('alert').last()).toBeVisible();
  await expect(page.getByRole('textbox', { name: bindT(lang)('build.code'), exact: true })).toHaveValue(source);
  expect(await capture(page)).toEqual(before);
}

async function screenshot(page: Page, target: Locator, testInfo: TestInfo, name: string) {
  await target.scrollIntoViewIfNeeded();
  await expect(target).toBeInViewport();
  const bounds = await target.boundingBox();
  expect(bounds).not.toBeNull();
  expect(bounds!.x).toBeGreaterThanOrEqual(0);
  expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(page.viewportSize()!.width);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await testInfo.attach(name, { body: await page.screenshot(), contentType: 'image/png' });
}

async function complete(page: Page, planner: Planner = 'route', lang: Lang = 'en-US') {
  await expect(run(page, planner, lang)).toBeEnabled();
  await run(page, planner, lang).click();
  const cards = planner === 'route' ? routeResults(page).locator(':scope > li') : joint(page).locator('.tree-planner-results > li');
  await expect(cards.first()).toBeVisible({ timeout: 90_000 });
  await expect(run(page, planner, lang)).toBeEnabled();
  await expect(page.locator('.opt-progress')).toHaveCount(0);
  return cards;
}

async function evaluateFrostRoutes(page: Page, lang: Lang) {
  await expect(run(page, 'route', lang)).toBeEnabled();
  await run(page, 'route', lang).click();
  const summary = page.locator('.tree-planner-summary');
  await expect(summary).toBeVisible({ timeout: 90_000 });
  await expect(run(page, 'route', lang)).toBeEnabled();
  await expect(page.locator('.opt-progress, .opt-error')).toHaveCount(0);
  const evaluated = Number((await summary.locator(':scope > span').first().innerText()).split(':').at(-1));
  expect(evaluated, 'The real imported build must finish actual route evaluations, even if none improves its objective').toBeGreaterThan(0);
}

async function preview(page: Page, card: Locator, planner: Planner, lang: Lang = 'en-US') {
  await card.getByRole('button', { name: planner === 'route' ? labels[lang].preview : 'Show joint route on tree', exact: true }).click();
  return {
    add: await page.locator('circle.node-planned-add[data-skill-id]').evaluateAll(elements => elements.map(e => Number(e.getAttribute('data-skill-id')))),
    remove: await page.locator('circle.node-planned-remove[data-skill-id]').evaluateAll(elements => elements.map(e => Number(e.getAttribute('data-skill-id')))),
  };
}

function normalizeAllocation(envelope: Envelope) {
  const copy = structuredClone(envelope);
  copy.state.allocatedNodes.sort((a, b) => a - b);
  for (const build of copy.workspace.builds) for (const stage of build.stages) stage.saved.state.allocatedNodes.sort((a, b) => a - b);
  return copy;
}

async function exactApply(page: Page, card: Locator, planner: Planner, before: Capture, lang: Lang = 'en-US') {
  const edit = await preview(page, card, planner, lang);
  expect(edit.add.length, 'Apply must make a real allocation edit').toBeGreaterThan(0);
  const expected = structuredClone(before.envelope);
  const state = expected.state;
  state.allocatedNodes = state.allocatedNodes.filter(id => !edit.remove.includes(id)).concat(edit.add);
  const choices: Record<string, AttributeChoice> = Object.fromEntries(Object.entries(state.attributeChoices)
    .filter(([id]) => state.allocatedNodes.includes(Number(id))));
  for (const id of edit.add) if (nodes.find(node => node.skill === id)?.name === 'Attribute') choices[String(id)] = 'str';
  state.attributeChoices = choices;
  if (planner === 'joint') state.jewels = [{ socket_node: socket.skill, text: jewel }];
  const activeBuild = expected.workspace.builds.find(build => build.id === expected.workspace.activeBuild)!;
  activeBuild.stages.find(stage => stage.id === activeBuild.activeStage)!.saved.state = state;
  const expectedRequest = requestOf(state);
  const expectedResult = await calculate(expectedRequest);
  const metrics = ['Life', 'TotalDPS', 'TotalEHP'] as const;
  for (const id of metrics) {
    const value = expectedResult.stats.find(stat => stat.id === id)!.value!;
    expect(Math.abs(numeric(await card.locator(`output[data-stat="${id}"]`).innerText()) - value),
      `${id} must match the independently replayed route to its displayed precision`).toBeLessThanOrEqual(0.00501);
  }
  await card.getByRole('button', { name: planner === 'route' ? labels[lang].apply : 'Apply jewel and passive plan', exact: true }).click();
  // The receipt belongs to the allocation that was just replaced, so it goes with it. A planner that keeps
  // tracking the tree searches again for the new allocation a moment later, hence the check comes first.
  await expect(results(page, planner)).toHaveCount(0);
  await expect.poll(async () => normalizeAllocation(await saved(page))).toEqual(normalizeAllocation(expected));
  const after = await capture(page);
  expect(after.bytes.library).toBe(before.bytes.library);
  expect({ ...after.request, allocated_nodes: [...after.request.allocated_nodes!].sort((a, b) => a - b) })
    .toEqual({ ...expectedRequest, allocated_nodes: [...expectedRequest.allocated_nodes!].sort((a, b) => a - b) });
  expect(after.result).toEqual(expectedResult);
  for (const id of metrics) {
    const row = STAT_SECTIONS.flatMap(section => section.rows).find(row => row.id === id)!;
    const value = expectedResult.stats.find(stat => stat.id === id)!.value;
    await expect(statValue(page, new RegExp(`^${row.label[lang]}$`)).first()).toHaveText(formatStatValue(value, row.format));
  }
  return after;
}

for (const lang of ['en-US', 'zh-TW', 'zh-CN'] as const) {
  for (const width of [390, 1440]) {
    test(`${lang} ${width}px: rejected imports preserve Frost Bomb and fresh route evaluation completes`, async ({ page }, testInfo) => {
      test.setTimeout(300_000);
      await page.setViewportSize({ width, height: 780 });
      await frostWorkspace(page, lang);
      await configure(page, lang);
      const before = await capture(page);
      await paste(page, JSON.stringify(before.envelope), 'currentStage', lang, true);
      await expect(page.getByRole('alert')).toHaveCount(0);
      expect(await capture(page)).toEqual(before);
      await reject(page, before, 'newBuild', lang);
      await screenshot(page, page.getByRole('alert').last(), testInfo, 'new-build-rejection');
      await openTree(page, lang);
      // Expected I15 red seam: the unchanged valid calculation must remain usable.
      await expect(run(page, 'route', lang)).toBeEnabled();
      await evaluateFrostRoutes(page, lang);
      expect(await capture(page)).toEqual(before);
      await reject(page, before, 'currentStage', lang);
      await screenshot(page, page.getByRole('alert').last(), testInfo, 'current-stage-rejection');
      await openTree(page, lang);
      await expect(page.getByRole('spinbutton', { name: labels[lang].points, exact: true })).toHaveValue('3');
      await expect(page.getByRole('combobox', { name: labels[lang].attribute, exact: true })).toContainText(labels[lang].strength);
      await evaluateFrostRoutes(page, lang);
      expect(await capture(page)).toEqual(before);
      await screenshot(page, page.locator('.tree-planner-summary'), testInfo, 'fresh-evaluated-route');
    });
  }
}

async function addPlacement(page: Page, text: string, lang: Lang = 'en-US') {
  await joint(page, lang).getByRole('textbox', { name: jointLabels[lang].text, exact: true }).fill(text);
  await joint(page, lang).getByRole('combobox', { name: jointLabels[lang].socket, exact: true }).click();
  await page.getByRole('option').filter({ hasText: `#${socket.skill}` }).click();
  await joint(page, lang).getByRole('button', { name: jointLabels[lang].add, exact: true }).click();
}

for (const planner of ['route', 'joint'] as const) {
  for (const destination of ['newBuild', 'currentStage'] as const) {
    test(`${planner} fresh planning after standalone rejected ${destination} completes and Applies only its evaluated edit`, async ({ page }) => {
      test.setTimeout(300_000);
      await syntheticWitch(page);
      await configure(page);
      if (planner === 'joint') await addPlacement(page, jewel);
      const before = await capture(page);
      await expect(routeResults(page)).toHaveCount(0);
      await expect(joint(page).locator('.jewel-plan-results')).toHaveCount(0);
      await reject(page, before, destination);
      await openTree(page);
      const cards = await complete(page, planner);
      expect(await capture(page)).toEqual(before);
      const selected = planner === 'route' ? cards.first() : cards.filter({ hasText: '+1000 to maximum Life' }).first();
      await exactApply(page, selected, planner, before);
    });
  }
}

for (const planner of ['route', 'joint'] as const) {
  test(`${planner}: compatible complete receipts survive cancellation and both rejected destinations, with exact attributed Apply`, async ({ page }) => {
    test.setTimeout(300_000);
    await syntheticWitch(page);
    await configure(page);
    await addPlacement(page, jewel);
    await addPlacement(page, unmodeledJewel);
    await joint(page).getByRole('textbox', { name: 'Candidate jewel text', exact: true }).fill(jewel);
    await complete(page, 'joint');
    await complete(page, 'route');
    const routeReceipt = await routeResults(page).innerText();
    const jointReceipt = await results(page, 'joint').innerText();
    const candidates = await joint(page).locator('.jewel-plan-candidates').innerText();
    const socketLabel = await joint(page).getByRole('combobox', { name: 'Candidate socket', exact: true }).innerText();
    const before = await capture(page);
    await paste(page, JSON.stringify(before.envelope), 'currentStage', 'en-US', true);
    expect(await capture(page)).toEqual(before);
    await expect(page.getByRole('alert')).toHaveCount(0);
    for (const destination of ['newBuild', 'currentStage'] as const) {
      await reject(page, before, destination);
      await pageLink(page, 'Calcs').click();
      await openTree(page);
      await expect.poll(() => routeResults(page).innerText()).toBe(routeReceipt);
      await expect.poll(() => results(page, 'joint').innerText()).toBe(jointReceipt);
      await expect(run(page, 'route')).toBeEnabled();
      await expect(run(page, 'joint')).toBeEnabled();
      await expect(page.getByRole('spinbutton', { name: 'Point budget', exact: true })).toHaveValue('3');
      await expect(page.getByRole('combobox', { name: 'New travel attributes', exact: true })).toContainText('Strength');
      await expect(joint(page).getByRole('textbox', { name: 'Candidate jewel text', exact: true })).toHaveValue(jewel);
      expect(await joint(page).getByRole('combobox', { name: 'Candidate socket', exact: true }).innerText()).toBe(socketLabel);
      expect(await joint(page).locator('.jewel-plan-candidates').innerText()).toBe(candidates);
      expect(await capture(page)).toEqual(before);
    }
    const cards = planner === 'route' ? routeResults(page).locator(':scope > li') : joint(page).locator('.tree-planner-results > li');
    let selected = cards.filter({ hasText: '+1000 to maximum Life' }).first();
    if (planner === 'route') {
      let indexWithAttribute = -1;
      for (let index = 0; index < await cards.count(); index++) {
        const edit = await preview(page, cards.nth(index), planner);
        if (edit.add.some(id => nodes.find(node => node.skill === id)?.name === 'Attribute')) { indexWithAttribute = index; break; }
      }
      expect(indexWithAttribute, 'Require a genuinely new Strength travel node alongside retained Intelligence choices').toBeGreaterThanOrEqual(0);
      selected = cards.nth(indexWithAttribute);
    }
    await exactApply(page, selected, planner, before);
  });
}

test('support receipts and settings survive rejected imports without losing the evaluated full-set Apply', async ({ page }) => {
  test.setTimeout(300_000);
  await syntheticWitch(page);
  await pageLink(page, 'Skills').click();
  const group = page.locator('.skill-group').first();
  if (await group.locator('.skill-group-title').getAttribute('aria-expanded') === 'false') await group.locator('.skill-group-title').click();
  if (await group.locator('.gem-optimizer-toggle').getAttribute('aria-expanded') === 'false') await group.locator('.gem-optimizer-toggle').click();
  await openSupportSettings(page);
  const capacity = page.getByRole('spinbutton', { name: 'Unlocked support sockets', exact: true });
  const lineage = page.getByRole('checkbox', { name: 'Include lineage supports', exact: true });
  await capacity.fill('3');
  await lineage.uncheck();
  const supportRun = page.getByRole('button', { name: 'Calculate support combinations', exact: true });
  await supportRun.click();
  await expect(page.locator('.support-plan').first()).toBeVisible({ timeout: 90_000 });
  await expect(supportRun).toBeEnabled();
  const receipt = await page.locator('.support-results').innerText();
  const before = await capture(page);
  for (const destination of ['newBuild', 'currentStage'] as const) {
    await reject(page, before, destination);
    await pageLink(page, 'Skills').click();
    // The settings section folds up when the page is left; the values in it are what must have been kept.
    await openSupportSettings(page);
    await expect(capacity).toHaveValue('3');
    await expect(lineage).not.toBeChecked();
    expect(await page.locator('.support-results').innerText()).toBe(receipt);
    await expect(supportRun).toBeEnabled();
    expect(await capture(page)).toEqual(before);
  }
  const first = page.locator('.support-plan').first();
  const supports = await first.locator('.support-plan-gems > .opt-chip').evaluateAll(elements => elements.map(element => {
    const statSet = element.getAttribute('data-stat-set-index');
    return { skill_id: element.getAttribute('data-skill-id')!,
      level: Number(element.getAttribute('data-level')), quality: Number(element.getAttribute('data-quality')),
      ...(statSet !== null ? { stat_set_index: Number(statSet) } : {}) };
  }));
  const previewText = await first.locator('.support-plan-metrics > span').filter({ hasText: /^DPS / }).innerText();
  const predictedDps = numeric(previewText.match(/^DPS ([\d,.]+)/)![1]);
  await first.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect.poll(async () => (await saved(page)).state.socketGroups[0].gems.slice(1)).toEqual(supports);
  const after = await capture(page);
  const expected = structuredClone(before.envelope);
  expected.state.socketGroups[0].gems = [before.envelope.state.socketGroups[0].gems[0], ...supports];
  const active = expected.workspace.builds.find(build => build.id === expected.workspace.activeBuild)!;
  active.stages.find(stage => stage.id === active.activeStage)!.saved.state = expected.state;
  expect(after.envelope).toEqual(expected);
  expect(after.envelope.state.socketGroups[0].gems[0]).toEqual(before.envelope.state.socketGroups[0].gems[0]);
  expect(after.bytes.library).toBe(before.bytes.library);
  const actualDps = after.result.stats.find(stat => stat.id === 'TotalDPS')!.value!;
  expect(actualDps).toBeGreaterThan(before.result.stats.find(stat => stat.id === 'TotalDPS')!.value!);
  expect(Math.abs(actualDps - predictedDps)).toBeLessThanOrEqual(0.06);
  await expect(statValue(page, /^Total DPS$/).first()).toHaveText(formatStatValue(actualDps, 'float2'));
  await expect(page.locator('.support-results')).toHaveCount(0);
});

test('strict saved-file rejection stays local and external decoder rejection also permits fresh real planning', async ({ page }) => {
  test.setTimeout(300_000);
  await syntheticWitch(page);
  await configure(page);
  const before = await capture(page);
  await openBuild(page);
  const damaged = { version: 1, notes: 'Saved identity wins over valid legacy collections', state: null, passives: [], items: [], skills: [] };
  await page.locator('input[type=file]').setInputFiles({ name: 'damaged-saved-session.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(damaged)) });
  await expect(page.getByRole('alert').last()).toBeVisible();
  expect(await capture(page)).toEqual(before);
  await openTree(page);
  await expect(run(page)).toBeEnabled();
  await complete(page);
  const receipt = await routeResults(page).innerText();
  await openBuild(page);
  await page.locator('input[type=file]').setInputFiles({ name: 'unrecognized-external.build', mimeType: 'application/json', buffer: Buffer.from('{}') });
  await expect(page.getByRole('alert').last()).toBeVisible();
  expect(await capture(page)).toEqual(before);
  await openTree(page);
  expect(await routeResults(page).innerText()).toBe(receipt);
  await expect(run(page)).toBeEnabled();
  const cards = await complete(page);
  await exactApply(page, cards.first(), 'route', before);
});

for (const lang of ['en-US', 'zh-TW', 'zh-CN'] as const) {
  for (const width of [390, 1440]) {
    test(`${lang} ${width}px: real failed calculation remains blocked across a later rejected import and recovers only after a valid edit`, async ({ page }, testInfo) => {
      test.setTimeout(300_000);
      await page.setViewportSize({ width, height: 780 });
      await syntheticWitch(page, lang);
      await configure(page, lang);
      await addPlacement(page, jewel, lang);
      await expect(run(page, 'joint', lang)).toBeEnabled();
      const candidates = await joint(page, lang).locator('.jewel-plan-candidates').innerText();
      const before = await capture(page);
      // Named uint32-overflow request boundary: the existing gem editor accepts
      // this positive integer; the real WASM decoder rejects it as out of range.
      // Do not repair editor bounds here or substitute a mocked engine error.
      await goTo(page, bindT(lang)('tab.skills'), lang);
      const group = page.locator('.skill-group').first();
      if (await group.locator('.skill-group-title').getAttribute('aria-expanded') === 'false') await group.locator('.skill-group-title').click();
      const level = group.locator('.skill-gem').first().getByRole('spinbutton', { name: bindT(lang)('skills.level'), exact: true });
      await level.fill('4294967296');
      const calcAlert = page.getByRole('alert').filter({ hasText: '4294967296' });
      await expect(calcAlert).toContainText('u32');
      // The heading and the engine message sit in separate blocks; textContent is what toHaveText compares against.
      const rawCalcError = (await calcAlert.textContent())!;
      await settled(page);
      const invalid = await saved(page);
      expect(invalid.state.socketGroups[0].gems[0].level).toBe(4294967296);
      const backend = await replayBackend();
      let engineError = '';
      try { backend.calculateBuildJson(JSON.stringify(requestOf(invalid.state))); } catch (error) { engineError = String(error); }
      expect(engineError).toContain('4294967296');
      expect(engineError).toContain('u32');
      const rejectedSource = JSON.stringify(before.envelope);
      let rawImportError = '';
      try { backend.decodeBuildFileJson(rejectedSource); } catch (error) { rawImportError = formatApiError(error); }
      expect(rawImportError).toContain('passives');
      await openTree(page, lang);
      await expect(run(page, 'route', lang)).toBeDisabled();
      await expect(run(page, 'joint', lang)).toBeDisabled();
      expect(await joint(page, lang).locator('.jewel-plan-candidates').innerText()).toBe(candidates);
      await expect(joint(page, lang).locator('.jewel-plan-results')).toHaveCount(0);
      await expect(routeResults(page)).toHaveCount(0);
      await screenshot(page, calcAlert, testInfo, 'real-calculation-failure');
      const failedBytes = await bytes(page);
      const failedDisplay = await displayed(page);
      await paste(page, rejectedSource, 'newBuild', lang);
      // A later operation error must not clear or replace the calculation failure.
      await expect(calcAlert).toHaveText(rawCalcError);
      const sourceAlert = page.getByRole('alert').filter({ hasText: rawImportError });
      await expect(sourceAlert).toHaveText(rawImportError);
      await expect(calcAlert).toBeVisible();
      await expect(sourceAlert).toBeVisible();
      await expect(page.getByRole('alert')).toHaveCount(2);
      expect(await bytes(page)).toEqual(failedBytes);
      expect(await displayed(page)).toEqual(failedDisplay);
      await openTree(page, lang);
      await expect(run(page, 'route', lang)).toBeDisabled();
      await expect(run(page, 'joint', lang)).toBeDisabled();
      expect(await joint(page, lang).locator('.jewel-plan-candidates').innerText()).toBe(candidates);
      await expect(joint(page, lang).locator('.jewel-plan-results')).toHaveCount(0);
      await expect(calcAlert).toHaveText(rawCalcError);
      await expect(sourceAlert).toHaveText(rawImportError);
      await expect(routeResults(page)).toHaveCount(0);
      await sourceAlert.scrollIntoViewIfNeeded();
      await expect(calcAlert).toBeInViewport();
      await expect(sourceAlert).toBeInViewport();
      await screenshot(page, calcAlert, testInfo, 'calculation-failure-after-rejected-import');
      await goTo(page, bindT(lang)('tab.skills'), lang);
      await level.fill(String(before.envelope.state.socketGroups[0].gems[0].level));
      await settled(page);
      await expect(page.getByRole('alert')).toHaveCount(0);
      const recovered = await capture(page);
      expect(recovered.request).toEqual(before.request);
      expect(recovered.result).toEqual(before.result);
      expect(recovered.displayed).toEqual(before.displayed);
      expect(recovered.bytes.library).toBe(before.bytes.library);
      expect(recovered.envelope).toEqual(before.envelope);
      await configure(page, lang);
      const cards = await complete(page, 'route', lang);
      await screenshot(page, cards.first().locator('.tree-plan-metrics'), testInfo, 'recovered-valid-calculation-route');
      await exactApply(page, cards.first(), 'route', recovered, lang);
    });
  }
}
