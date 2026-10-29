import { expect, test } from '@playwright/test';
import type { Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { deflateSync } from 'node:zlib';
import type { CalculateBuildRequest, CalculateBuildResponse, PassiveNode } from '../src/api/types';
import type { SavedSession } from '../src/hooks/useBuildSession';
import { bindT } from '../src/lib/i18n';
import { chineseScript } from '../src/lib/chinese';
import type { Lang } from '../src/lib/i18n';
import { allocationAccess, buildPassiveGraph, shortestAllocationPath } from '../src/lib/passiveGraph';
import * as wasmReplay from '../src/wasm/pkg/pobr_wasm.js';
import { busy, pageLink } from './shell';
import { answerConfirm } from './confirmDialog';
import { choose, newControlContext, setLanguage } from './treePage';

const fixture = readFileSync(new URL('../../examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt', import.meta.url), 'utf8').trim();
const version = readFileSync(new URL('../../data/CURRENT', import.meta.url), 'utf8').trim();
const nodes: PassiveNode[] = JSON.parse(readFileSync(new URL(`../../data/${version}/base/passive_tree.json`, import.meta.url), 'utf8'));
const nav = (page: Page, name: string) => pageLink(page, name);
const saved = (page: Page): Promise<SavedSession['state']> => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state);
const savedBytes = (page: Page) => page.evaluate(() => localStorage.getItem('pobr-v2-build-state'));

// Main's actual old-pointer control found this first Spell Damage result. Keep
// the committed build intact; this is not a synthetic numerical oracle.
const target = nodes.find(node => node.skill === 511)!;
const attribute = nodes.find(node => node.skill === 3251)!;
const catalog = JSON.parse(readFileSync(new URL(`../../data/${version}/overlay/passive_jewels.json`, import.meta.url), 'utf8'));
const witchRoot = nodes.find(node => node.id === catalog.class_starts.witch)!.skill;
const starts = new Set<string>(Object.values(catalog.class_starts));
const graph = buildPassiveGraph(nodes.filter(node => !node.ascendancy_id && (!starts.has(node.id) || node.skill === witchRoot)));
const socket = nodes.filter(node => node.kind === 'jewel_socket' && node.id.startsWith('jewel_slot'))
  .sort((a, b) => shortestAllocationPath(graph, new Set(), witchRoot, a.skill).length
    - shortestAllocationPath(graph, new Set(), witchRoot, b.skill).length || a.skill - b.skill)[0];

let replayPromise: Promise<typeof wasmReplay> | undefined;

async function replayBackend() {
  if (!replayPromise) replayPromise = (async () => {
    // Separate Node WASM instance, initialized with the same shipped snapshot.
    // No production backend monkey patch, scoring mock or duplicated formula.
    const wasm = wasmReplay;
    await wasm.default({ module_or_path: readFileSync(new URL('../src/wasm/pkg/pobr_wasm_bg.wasm', import.meta.url)) });
    const manifest: { version: string; files: string[] } = JSON.parse(
      readFileSync(new URL('../public/data/manifest.json', import.meta.url), 'utf8'));
    expect(manifest.version).toBe(version);
    for (const file of manifest.files) wasm.stageDataFile(file,
      readFileSync(new URL(`../public/data/${version}/${file}`, import.meta.url), 'utf8'));
    wasm.initStagedData();
    return wasm;
  })();
  return replayPromise;
}

async function capture(page: Page) {
  await idle(page);
  const state = await saved(page);
  // Independent projection of the persisted host contract, including weapon
  // group enablement; do not call useBuildSession's private materializer.
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
  const wasm = await replayBackend();
  const result: CalculateBuildResponse = JSON.parse(wasm.calculateBuildJson(JSON.stringify(request)));
  const displayed = await page.locator('#character-stats .rail-row').evaluateAll(rows =>
    rows.map(row => ({ label: row.querySelector('.rail-label')?.textContent, value: row.querySelector('.rail-value')?.textContent })));
  return { state, request: JSON.parse(JSON.stringify(request)), result, displayed };
}

const language = (page: Page, lang: Lang) => setLanguage(page, lang);

async function keyboardResult(page: Page, query: string, skill: number, lang: Lang = 'en-US') {
  const tt = bindT(lang);
  const search = page.getByRole('searchbox', { name: tt('tree.search'), exact: true });
  // A changed query deliberately resets result cycling and dismisses stale details.
  await search.fill('');
  await search.fill(query);
  const next = page.getByRole('button', { name: tt('tree.nextHit'), exact: true });
  const details = page.getByRole('group', { name: tt('tree.nodeDetails'), exact: true });
  const hits = await page.locator('circle.node-search-hit').count();
  expect(hits, `Real catalog must expose ${query}`).toBeGreaterThan(0);
  for (let index = 0; index < hits; index++) {
    await next.focus();
    await next.press('Enter');
    await expect(details).toBeVisible();
    await expect(details).toBeFocused();
    if (await details.getAttribute('data-skill-id') === String(skill)) return details;
  }
  throw new Error(`Search ${query} did not expose passive #${skill}`);
}

async function pointerNode(page: Page, skill: number) {
  // Existing E2E convention dispatches on the actual SVG node handler. Main also
  // owns real pixel-pointer evidence; this control is not an inspector click.
  const rect = await page.locator('.tree-canvas svg').boundingBox();
  await page.locator(`circle[data-skill-id="${skill}"]`).dispatchEvent('click', {
    clientX: rect!.x + Math.min(150, rect!.width / 2), clientY: rect!.y + Math.min(150, rect!.height / 2),
  });
  await idle(page);
}

async function seeded(page: Page, patch: Partial<SavedSession['state']> = {}) {
  const allocatedNodes = shortestAllocationPath(graph, new Set(), witchRoot, socket.skill);
  const state: SavedSession['state'] = {
    pobCode: null, treeVersion: `0_${version.split('.')[1]}`,
    character: { level: 90, class_name: 'Witch', ascendancy_name: '' },
    allocatedNodes, attributeChoices: Object.fromEntries(nodes
      .filter(node => node.name === 'Attribute' && allocatedNodes.includes(node.skill)).map(node => [String(node.skill), 'int' as const])),
    socketGroups: [{ enabled: true, gems: [{ skill_id: 'FireballPlayer', level: 16, quality: 0 }] }],
    items: [{ slot: 'ring1', text: 'Rarity: RARE\nSynthetic Keyboard Boundary Ring\nSapphire Ring\nImplicits: 0\n+80 to maximum Life' }],
    jewels: [{ socket_node: socket.skill, text: 'Rarity: MAGIC\nRuby\nImplicits: 0\n+1 to maximum Life' }],
    flasks: [], annotations: {}, params: { config_inputs: {} }, ...patch,
  };
  // Named synthetic persistence fixture only. Runtime nodes/graph/calculation and
  // planner evaluation remain shipped WASM/data. Reload never overwrites edits.
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  // The results panel eases numbers to their new value; read them as settled values.
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.addInitScript(state => {
    if (localStorage.getItem('pobr-v2-build-state')) return;
    localStorage.setItem('pobr-v2-build-state', JSON.stringify({ version: 1, notes: 'Synthetic keyboard boundary fixture', state }));
  }, state);
  await page.goto('/tree');
  await expect(page.locator('.tree-canvas')).toBeVisible({ timeout: 90_000 });
  await expect(page.locator('.tree-asc-picker select').first()).toBeEnabled();
  await idle(page);
}

async function idle(page: Page) {
  await expect(busy(page)).toHaveCount(0);
  await expect(page.locator('.shell-save-state')).not.toHaveClass(/is-error/);
  await expect(page.locator('.shell-boot[role=alert], .shell-alert')).toHaveCount(0);
  // The results panel keeps showing the previous numbers until the new calculation has settled.
  await expect(page.locator('#character-stats[data-stale]')).toHaveCount(0);
}

async function imported(page: Page, code = fixture) {
  // Only unrelated league discovery is intercepted; import/data/calculation are real.
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.goto('/build');
  await expect(page.getByRole('textbox', { name: 'Build code', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();
  await expect.poll(async () => (await saved(page)).pobCode).toBe(code);
  await idle(page);
  await nav(page, 'Tree').click();
  await expect(page.locator('.tree-canvas')).toBeVisible();
}

test('committed current-tree keyboard search opens actual node decisions without changing saved bytes', async ({ page }) => {
  await imported(page);
  const before = await savedBytes(page);
  const search = page.getByRole('searchbox', { name: 'Search nodes…', exact: true });
  await search.fill('Spell Damage');
  await search.press('Tab');
  const next = page.getByRole('button', { name: 'Next', exact: true });
  await expect(next).toBeFocused();
  await next.press('Enter');
  await expect(page.getByRole('group', { name: 'Node details', exact: true })).toBeVisible();
  expect(await savedBytes(page)).toBe(before);
});

// Editing needs unspent points: the committed level-98 fixture is already full.
// Keep the original-import test above, and exercise mutations on this explicit
// connected, funded fixture with real WASM skills, jewels and calculations.
async function funded(page: Page) {
  const allocatedNodes = [...new Set([
    ...shortestAllocationPath(graph, new Set(), witchRoot, socket.skill),
    ...shortestAllocationPath(graph, new Set(), witchRoot, attribute.skill),
  ])];
  await seeded(page, { allocatedNodes,
    attributeChoices: Object.fromEntries(allocatedNodes.filter(id => nodes.find(node => node.skill === id)?.name === 'Attribute')
      .map(id => [String(id), 'int' as const])),
  });
}

for (const lang of ['en-US', 'zh-TW', 'zh-CN'] as const) {
  for (const width of [1440, 390]) {
    test(`${lang} ${width}px: keyboard allocation refund and explicit attribute choice equal independent pointer state request and real result`, async ({ page, browser }) => {
      test.setTimeout(300_000);
      await page.setViewportSize({ width, height: 780 });
      await funded(page);
      await language(page, lang);
      const tt = bindT(lang);
      const original = await capture(page);
      expect(original.state.allocatedNodes).not.toContain(target.skill);
      expect(original.state.allocatedNodes).toContain(attribute.skill);
      const beforeBytes = await savedBytes(page);

      const controlContext = await newControlContext(browser, { width, height: 780 });
      try {
        const pointer = await controlContext.newPage();
        await funded(pointer);
        await language(pointer, lang);
        expect(await capture(pointer)).toEqual(original);
        await pointerNode(pointer, target.skill);
        await expect.poll(async () => (await saved(pointer)).allocatedNodes.includes(target.skill)).toBe(true);
        const pointerAllocated = await capture(pointer);
        const expectedRoute = shortestAllocationPath(graph, new Set(original.state.allocatedNodes), witchRoot, target.skill);
        expect(pointerAllocated.state.allocatedNodes).toEqual([
          ...original.state.allocatedNodes, ...expectedRoute,
        ].sort((a, b) => a - b));
        await pointerNode(pointer, target.skill);
        await expect.poll(async () => (await saved(pointer)).allocatedNodes.includes(target.skill)).toBe(false);
        const pointerRefunded = await capture(pointer);
        await pointer.locator(`circle[data-skill-id="${attribute.skill}"]`).dispatchEvent('pointerover',
          { clientX: 600, clientY: 400 });
        await pointer.locator('body').click({ position: { x: 2, y: 2 } });
        await pointer.keyboard.press('s');
        await expect.poll(async () => (await saved(pointer)).attributeChoices[String(attribute.skill)]).toBe('str');
        const pointerChoice = await capture(pointer);

        let details = await keyboardResult(page, 'Spell Damage', target.skill, lang);
        const changed = original.result.tree_effects?.nodes[String(target.skill)];
        const raw = [changed?.name ?? target.name!,
          ...(changed?.replace ? changed.stats : [...(target.stats ?? []), ...(changed?.stats ?? [])])].map(line =>
          line.replace(/\[([^\]|]*)\|([^\]]*)\]/g, '$2').replace(/\[([^\]]*)\]/g, '$1'));
        // The game-text layer translates to Simplified; Traditional is that translation converted by script.
        const expectedText: string[] = lang === 'en-US' ? raw
          : (JSON.parse((await replayBackend()).translateLinesToZhCn(JSON.stringify(raw))) as string[])
            .map(line => chineseScript(line, lang));
        await expect(details.getByRole('heading')).toHaveText(expectedText[0]);
        await expect(details.locator('.tree-node-details-stats > li')).toHaveText(expectedText.slice(1));
        await expect(details.locator('.tree-node-details-state')).toContainText(tt('tree.notAllocated'));
        const view = await page.locator('.tree-canvas svg').getAttribute('viewBox');
        await page.getByRole('button', { name: tt('tree.nextHit'), exact: true }).focus();
        await page.keyboard.press('Enter');
        await expect(details).toBeFocused();
        expect(await page.locator('.tree-canvas svg').getAttribute('viewBox')).not.toBe(view);
        expect(await savedBytes(page)).toBe(beforeBytes);
        await details.press('Tab');
        const close = details.getByRole('button', { name: tt('tree.closeDetails'), exact: true });
        await expect(close).toBeFocused();
        await close.press('Enter');
        await expect(details).toHaveCount(0);
        await expect(page.getByRole('button', { name: tt('tree.nextHit'), exact: true })).toBeFocused();
        expect(await savedBytes(page)).toBe(beforeBytes);
        details = await keyboardResult(page, 'Spell Damage', target.skill, lang);
        await details.press('Escape');
        await expect(details).toHaveCount(0);
        expect(await savedBytes(page)).toBe(beforeBytes);
        details = await keyboardResult(page, 'Spell Damage', target.skill, lang);
        await page.getByRole('searchbox', { name: tt('tree.search'), exact: true }).fill('no-such-keyboard-passive');
        await expect(details).toHaveCount(0);
        await expect(page.getByRole('button', { name: tt('tree.nextHit'), exact: true })).toHaveCount(0);
        expect(await savedBytes(page)).toBe(beforeBytes);

        details = await keyboardResult(page, 'Spell Damage', target.skill, lang);
        await expect(details.getByRole('button', { name: tt('tree.allocate'), exact: true })).toBeEnabled();
        await details.press('Tab');
        await page.keyboard.press('Tab');
        const allocate = details.getByRole('button', { name: tt('tree.allocate'), exact: true });
        await expect(allocate).toBeFocused();
        await expect(allocate).toBeEnabled();
        // Capture the actual visible inspector rather than a full-page image
        // whose nested scroller can hide the changed surface at narrow widths.
        await details.scrollIntoViewIfNeeded();
        await page.screenshot({ path: test.info().outputPath(`${lang}-${width}-node-inspector.png`) });
        expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
        await allocate.press('Enter');
        await expect.poll(async () => (await saved(page)).allocatedNodes.includes(target.skill)).toBe(true);
        expect(await capture(page)).toEqual(pointerAllocated);
        await expect(details.locator('.tree-node-details-state')).toContainText(tt('tree.allocated'));
        const newAttributes = nodes.filter(node => node.name === 'Attribute'
          && pointerAllocated.state.allocatedNodes.includes(node.skill) && !original.state.allocatedNodes.includes(node.skill));
        for (const node of newAttributes) expect(pointerAllocated.state.attributeChoices[String(node.skill)]).toBeUndefined();
        const refund = details.getByRole('button', { name: tt('tree.refund'), exact: true });
        await refund.focus();
        await refund.press('Enter');
        await expect.poll(async () => (await saved(page)).allocatedNodes.includes(target.skill)).toBe(false);
        expect(await capture(page)).toEqual(pointerRefunded);

        details = await keyboardResult(page, 'Attribute', attribute.skill, lang);
        await details.press('Tab');
        await page.keyboard.press('Tab');
        const choice = details.getByRole('combobox', { name: tt('tree.attributeChoice'), exact: true });
        await expect(choice).toBeFocused();
        await choose(page, choice, tt('tree.attr.str'));
        expect(await capture(page)).toEqual(pointerRefunded);
        const apply = details.getByRole('button', { name: tt('tree.applyAttribute'), exact: true });
        await choice.press('Tab');
        await expect(apply).toBeFocused();
        await apply.press('Enter');
        await expect.poll(async () => (await saved(page)).attributeChoices[String(attribute.skill)]).toBe('str');
        expect(await capture(page)).toEqual(pointerChoice);
        expect((await saved(page)).allocatedNodes).toEqual(pointerRefunded.state.allocatedNodes);
        await page.reload();
        await expect(page.locator('.tree-canvas')).toBeVisible({ timeout: 90_000 });
        await expect(page.getByRole('group', { name: tt('tree.nodeDetails'), exact: true })).toHaveCount(0);
        expect(await capture(page)).toEqual(pointerChoice);
        await test.info().attach('pointer-keyboard-complete-controls.json', {
          body: JSON.stringify({ original, allocated: pointerAllocated, refunded: pointerRefunded, choice: pointerChoice }, null, 2),
          contentType: 'application/json',
        });
      } finally {
        await controlContext.close();
      }
    });
  }
}

test('keyboard selection and drafts retain real planner receipts until explicit mutation; copied stages and reload isolate edits', async ({ page }) => {
  test.setTimeout(300_000);
  await seeded(page);
  const toggle = page.getByRole('button', { name: 'Plan passive upgrades', exact: true });
  await toggle.click();
  await page.getByRole('combobox', { name: 'Objective', exact: true }).click();
  await page.getByRole('option', { name: 'Max Life', exact: true }).click();
  await page.getByRole('spinbutton', { name: 'Point budget', exact: true }).fill('3');
  await page.getByRole('combobox', { name: 'New travel attributes', exact: true }).click();
  await page.getByRole('option', { name: 'Strength', exact: true }).click();
  await page.getByRole('button', { name: 'Find upgrade routes', exact: true }).click();
  const receipts = page.locator('.tree-planner-body > .tree-planner-results');
  await expect(receipts).toBeVisible({ timeout: 90_000 });
  await expect(page.getByRole('button', { name: 'Apply plan', exact: true }).first()).toBeEnabled();
  const joint = page.getByRole('region', { name: 'Jewels and passive routes', exact: true });
  await joint.getByRole('textbox', { name: 'Candidate jewel text', exact: true })
    .fill('Rarity: MAGIC\nRuby\nImplicits: 0\n+1000 to maximum Life');
  await joint.getByRole('combobox', { name: 'Candidate socket', exact: true }).click();
  await page.getByRole('option').filter({ hasText: `#${socket.skill}` }).click();
  await joint.getByRole('button', { name: 'Add jewel placement', exact: true }).click();
  await joint.getByRole('button', { name: 'Find jewel and passive plans', exact: true }).click();
  const jointResults = joint.locator('.jewel-plan-results');
  await expect(jointResults).toBeVisible({ timeout: 90_000 });
  const jointReceipt = await jointResults.innerText();
  const receipt = await receipts.innerText();
  const original = await capture(page);
  const attrs = nodes.filter(node => node.name === 'Attribute' && original.state.allocatedNodes.includes(node.skill))
    .sort((a, b) => a.skill - b.skill);
  expect(attrs.length, 'Fixture must supply an allocated attribute decision').toBeGreaterThan(0);
  const attr = attrs[0];
  let details = await keyboardResult(page, 'Attribute', attr.skill);
  await choose(page, details.getByRole('combobox', { name: 'Attribute choice', exact: true }), 'Strength');
  expect(await capture(page)).toEqual(original);
  await expect.poll(() => receipts.innerText()).toBe(receipt);
  await expect.poll(() => jointResults.innerText()).toBe(jointReceipt);
  await details.press('Escape');
  await nav(page, 'Calcs').click();
  await expect(page.locator('.tree-canvas')).toHaveCount(0);
  await nav(page, 'Tree').click();
  await expect.poll(() => receipts.innerText()).toBe(receipt);
  await expect.poll(() => jointResults.innerText()).toBe(jointReceipt);
  expect(await capture(page)).toEqual(original);
  details = await keyboardResult(page, 'Attribute', attr.skill);
  await expect(details.getByRole('combobox', { name: 'Attribute choice', exact: true })).toHaveText('Intelligence');
  await choose(page, details.getByRole('combobox', { name: 'Attribute choice', exact: true }), 'Strength');
  await details.getByRole('button', { name: 'Apply choice', exact: true }).focus();
  await page.keyboard.press('Enter');
  await expect.poll(async () => (await saved(page)).attributeChoices[String(attr.skill)]).toBe('str');
  await expect(receipts).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Apply plan', exact: true })).toHaveCount(0);
  await expect(jointResults).toHaveCount(0);
  await expect(joint.getByRole('button', { name: 'Apply jewel and passive plan', exact: true })).toHaveCount(0);
  const base = await capture(page);
  await page.locator('.shell-builds-button').click();
  const stagePicker = page.getByRole('combobox', { name: 'Current stage', exact: true });
  const originalStage = (await stagePicker.innerText()).trim();
  await page.getByRole('button', { name: 'Copy current stage', exact: true }).click();
  await page.getByRole('textbox', { name: 'Name', exact: true }).fill('Keyboard manual decisions');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await idle(page);
  await nav(page, 'Tree').click();
  await expect(page.getByRole('group', { name: 'Node details', exact: true })).toHaveCount(0);
  details = await keyboardResult(page, 'Attribute', attr.skill);
  await choose(page, details.getByRole('combobox', { name: 'Attribute choice', exact: true }), 'Dexterity');
  await details.getByRole('button', { name: 'Apply choice', exact: true }).focus();
  await page.keyboard.press('Enter');
  await expect.poll(async () => (await saved(page)).attributeChoices[String(attr.skill)]).toBe('dex');
  expect(base.state.allocatedNodes).not.toContain(target.skill);
  details = await keyboardResult(page, 'Spell Damage', target.skill);
  await details.getByRole('button', { name: 'Allocate', exact: true }).focus();
  await page.keyboard.press('Enter');
  await expect.poll(async () => (await saved(page)).allocatedNodes.includes(target.skill)).toBe(true);
  const copied = await capture(page);
  await choose(page, stagePicker, originalStage);
  await idle(page);
  await expect(page.getByRole('group', { name: 'Node details', exact: true })).toHaveCount(0);
  expect(await capture(page)).toEqual(base);
  await page.reload();
  await expect(page.locator('.tree-canvas')).toBeVisible({ timeout: 90_000 });
  expect(await capture(page)).toEqual(base);
  await choose(page, stagePicker, 'Keyboard manual decisions');
  await idle(page);
  expect(await capture(page)).toEqual(copied);
  await page.reload();
  await expect(page.locator('.tree-canvas')).toBeVisible({ timeout: 90_000 });
  expect(await capture(page)).toEqual(copied);
});

test('class confirmation and ascendancy replacement retire selected actions; current class start remains read-only', async ({ page }) => {
  await seeded(page);
  const original = await capture(page);
  let details = await keyboardResult(page, 'WITCH', witchRoot);
  await expect(details.getByRole('button', { name: 'Allocate', exact: true })).toBeDisabled();
  await pointerNode(page, witchRoot);
  expect(await capture(page)).toEqual(original);
  const classes = page.locator('.tree-asc-picker').first().getByRole('combobox');
  await choose(page, classes, 'Ranger');
  await answerConfirm(page, false);
  await expect(classes).toHaveText('Witch');
  expect(await capture(page)).toEqual(original);
  // Ascendancy replacement is an actual character edit, not a refusal guessed
  // from planner scope. It must remove any action bound to the previous context.
  await choose(page, page.locator('.tree-asc-picker').nth(1).getByRole('combobox'), 'Blood Mage');
  await expect.poll(async () => (await saved(page)).character.ascendancy_name).toBe('Blood Mage');
  await expect(details).toHaveCount(0);
  const blood = await saved(page);
  expect(blood.allocatedNodes).toEqual(original.state.allocatedNodes);
  expect(blood.attributeChoices).toEqual(original.state.attributeChoices);
  const selectedAsc = nodes.find(node => node.ascendancy_id === 'Witch2' && node.name && node.kind === 'notable')!;
  details = await keyboardResult(page, selectedAsc.name!, selectedAsc.skill);
  await expect(details.locator('.tree-node-details-state')).toContainText(`#${selectedAsc.skill}`);
  // Other ascendancies are not rendered/searchable, matching pointer canvas scope.
  const foreignAsc = nodes.find(node => node.ascendancy_id === 'Witch1' && node.kind === 'notable')!;
  await expect(page.locator(`circle[data-skill-id="${foreignAsc.skill}"]`)).toHaveCount(0);
  await choose(page, classes, 'Ranger');
  await answerConfirm(page, true);
  await expect.poll(async () => (await saved(page)).character.class_name).toBe('Ranger');
  await idle(page);
  await expect(details).toHaveCount(0);
  const reset = await saved(page);
  expect(reset.character).toEqual({ level: 1, class_name: 'Ranger', ascendancy_name: '' });
  expect(reset.allocatedNodes).toEqual([]);
  expect(reset.attributeChoices).toEqual({});
  expect(reset.socketGroups).toEqual([]);
  expect(reset.items).toEqual([]);
  expect(reset.jewels).toEqual([]);
});

test('historical replacement keeps exact version and calculation but exposes no stale keyboard node action', async ({ page }) => {
  await seeded(page);
  await keyboardResult(page, 'WITCH', witchRoot);
  await page.locator('.shell-builds-button').click();
  const code = deflateSync('<PathOfBuilding2><Build level="80" className="Witch"/><Tree activeSpec="1"><Spec nodes="770" treeVersion="0_1"/></Tree><Skills/><Items/></PathOfBuilding2>').toString('base64url');
  await page.getByRole('group', { name: 'Import into', exact: true }).getByRole('button', { name: /^Replace current stage/ }).click();
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();
  await answerConfirm(page, true);
  await expect.poll(async () => (await saved(page)).treeVersion).toBe('0_1');
  await idle(page);
  await nav(page, 'Tree').click();
  await expect(page.getByRole('main').getByText('Interactive tree editing and route planning are unavailable for historical trees.', { exact: false })).toBeVisible();
  await expect(page.locator('.tree-canvas, .tree-node-details, .tree-planner')).toHaveCount(0);
  const historical = await capture(page);
  await page.locator('body').click({ position: { x: 2, y: 2 } });
  // The number keys are page shortcuts in this shell; they leave the Tree page, so come back to it after them.
  for (const key of ['s', 'd', 'i', '1', '2', '3', 'Enter']) await page.keyboard.press(key);
  expect(await capture(page)).toEqual(historical);
  await nav(page, 'Tree').click();
  await expect(page.getByRole('main').getByText('Interactive tree editing and route planning are unavailable for historical trees.', { exact: false })).toBeVisible();
  await page.reload();
  await expect(page.getByRole('main').getByText('Interactive tree editing and route planning are unavailable for historical trees.', { exact: false })).toBeVisible({ timeout: 90_000 });
  expect(await capture(page)).toEqual(historical);
});

test('weapon-exclusive refund equals pointer cleanup and changing active set cannot reuse selected node actions', async ({ page, browser }) => {
  const baseNodes = [...new Set([
    ...shortestAllocationPath(graph, new Set(), witchRoot, socket.skill),
    ...shortestAllocationPath(graph, new Set(), witchRoot, target.skill).slice(0, -1),
    ...shortestAllocationPath(graph, new Set(), witchRoot, 116).slice(0, -1),
  ])];
  const patch: Partial<SavedSession['state']> = {
    allocatedNodes: [...baseNodes, target.skill],
    weaponSwap: { active: 1, alternate_items: [], exclusive_nodes: [[target.skill], [116]] },
  };
  await seeded(page, patch);
  const controlContext = await newControlContext(browser);
  try {
    const pointer = await controlContext.newPage();
    await seeded(pointer, patch);
    await pointerNode(pointer, target.skill);
    await expect.poll(async () => (await saved(pointer)).allocatedNodes.includes(target.skill)).toBe(false);
    const expected = await capture(pointer);
    const details = await keyboardResult(page, 'Spell Damage', target.skill);
    await details.getByRole('button', { name: 'Refund', exact: true }).focus();
    await page.keyboard.press('Enter');
    await expect.poll(async () => (await saved(page)).allocatedNodes.includes(target.skill)).toBe(false);
    expect(await capture(page)).toEqual(expected);
    expect(expected.state.weaponSwap?.exclusive_nodes).toEqual([[], [116]]);
    await nav(page, 'Items').click();
    await page.getByRole('group', { name: 'Active weapon set', exact: true }).getByRole('button', { name: 'Set 2', exact: true }).click();
    await expect.poll(async () => (await saved(page)).weaponSwap?.active).toBe(2);
    await nav(page, 'Tree').click();
    await expect(details).toHaveCount(0);
    const alternate = await capture(page);
    expect(alternate.state.allocatedNodes).toContain(116);
    expect(alternate.state.allocatedNodes).not.toContain(target.skill);
    expect(alternate.state.weaponSwap?.exclusive_nodes).toEqual([[], [116]]);
    await page.reload();
    await expect(page.locator('.tree-canvas')).toBeVisible({ timeout: 90_000 });
    expect(await capture(page)).toEqual(alternate);
    await nav(page, 'Items').click();
    await page.getByRole('group', { name: 'Active weapon set', exact: true }).getByRole('button', { name: 'Set 1', exact: true }).click();
    await expect.poll(async () => (await saved(page)).weaponSwap?.active).toBe(1);
    await nav(page, 'Tree').click();
    expect(await capture(page)).toEqual(expected);
  } finally {
    await controlContext.close();
  }
});

test('focused inspector controls and hidden canvas cannot receive stale pointer-hover attribute shortcuts', async ({ page }) => {
  await imported(page);
  const details = await keyboardResult(page, 'Spell Damage', target.skill);
  await page.locator(`circle[data-skill-id="${attribute.skill}"]`).dispatchEvent('pointerover', { clientX: 600, clientY: 400 });
  await details.getByRole('button', { name: 'Close details', exact: true }).focus();
  const original = await capture(page);
  // While the tree is showing, only S/D/I are attribute shortcuts: the number keys jump between pages in this shell.
  const attributeKeys = ['s', 'd', 'i'];
  for (const key of attributeKeys) await page.keyboard.press(key);
  expect(await capture(page)).toEqual(original);
  await details.press('Escape');
  await expect(page.getByRole('button', { name: 'Next', exact: true })).toBeFocused();
  for (const key of attributeKeys) {
    await page.keyboard.press(key);
    expect(await capture(page)).toEqual(original);
  }
  await page.locator(`circle[data-skill-id="${attribute.skill}"]`).dispatchEvent('pointerover', { clientX: 600, clientY: 400 });
  // The side column is open by default; closing it and reopening it from the keyboard moves focus into the tools.
  const sideToggle = page.getByRole('button', { name: 'Attributes & planning', exact: true });
  if (await sideToggle.getAttribute('aria-expanded') === 'true') await sideToggle.click();
  await expect(sideToggle).toHaveAttribute('aria-expanded', 'false');
  await sideToggle.focus();
  await page.keyboard.press('Enter');
  await expect(page.locator('.tree-edit-tools')).toBeFocused();
  for (const key of attributeKeys) {
    await page.keyboard.press(key);
    expect(await capture(page)).toEqual(original);
  }
  await nav(page, 'Calcs').dispatchEvent('click');
  await expect(page.locator('.tree-canvas, .tree-node-details, .tree-tooltip')).toHaveCount(0);
  await page.locator('body').click({ position: { x: 2, y: 2 } });
  for (const key of ['s', 'd', 'i', '1', '2', '3']) await page.keyboard.press(key);
  expect(await capture(page)).toEqual(original);
  await nav(page, 'Tree').click();
  await expect(page.getByRole('group', { name: 'Node details', exact: true })).toHaveCount(0);
  expect(await capture(page)).toEqual(original);
});

test('adjacent unallocated Attribute draft and cancellation are read-only; explicit allocation and refund equal pointer chooser', async ({ page, browser }) => {
  const attr = nodes.find(node => node.skill === 722)!;
  const route = shortestAllocationPath(graph, new Set(), witchRoot, attr.skill);
  const patch: Partial<SavedSession['state']> = {
    allocatedNodes: route.slice(0, -1).sort((a, b) => a - b), attributeChoices: {}, jewels: [],
  };
  await seeded(page, patch);
  const original = await capture(page);
  const controlContext = await newControlContext(browser);
  try {
    const pointer = await controlContext.newPage();
    await seeded(pointer, patch);
    await pointerNode(pointer, attr.skill);
    await expect(pointer.locator('.attr-picker')).toBeVisible();
    expect(await capture(pointer)).toEqual(original);
    await pointer.getByRole('menuitem', { name: /^Strength/ }).click();
    await expect.poll(async () => (await saved(pointer)).attributeChoices[String(attr.skill)]).toBe('str');
    const allocated = await capture(pointer);
    await pointerNode(pointer, attr.skill);
    await expect.poll(async () => (await saved(pointer)).allocatedNodes.includes(attr.skill)).toBe(false);
    const refunded = await capture(pointer);

    let details = await keyboardResult(page, 'Attribute', attr.skill);
    const allocate = details.getByRole('button', { name: 'Allocate', exact: true });
    await expect(allocate).toBeDisabled();
    const choice = details.getByRole('combobox', { name: 'Attribute choice', exact: true });
    await choose(page, choice, 'Strength');
    await expect(allocate).toBeEnabled();
    expect(await capture(page)).toEqual(original);
    await details.press('Escape');
    details = await keyboardResult(page, 'Attribute', attr.skill);
    await expect(details.getByRole('combobox', { name: 'Attribute choice', exact: true })).toHaveText('unassigned');
    await expect(allocate).toBeDisabled();
    expect(await capture(page)).toEqual(original);
    await choose(page, details.getByRole('combobox', { name: 'Attribute choice', exact: true }), 'Strength');
    await allocate.focus();
    await allocate.press('Enter');
    await expect.poll(async () => (await saved(page)).attributeChoices[String(attr.skill)]).toBe('str');
    expect(await capture(page)).toEqual(allocated);
    await details.getByRole('button', { name: 'Refund', exact: true }).focus();
    await page.keyboard.press('Enter');
    await expect.poll(async () => (await saved(page)).allocatedNodes.includes(attr.skill)).toBe(false);
    expect(await capture(page)).toEqual(refunded);
    expect(refunded.state.attributeChoices).toEqual({});
    expect(refunded.state.allocatedNodes).toEqual(original.state.allocatedNodes);
  } finally {
    await controlContext.close();
  }
});

test('remote Attribute hover shortcuts allocate a connected route with an explicit choice', async ({ page }) => {
  await seeded(page, { allocatedNodes: [], attributeChoices: {}, jewels: [] });
  const remote = nodes.find(node => node.skill === 722)!;
  const route = shortestAllocationPath(graph, new Set(), witchRoot, remote.skill);
  await page.locator(`circle[data-skill-id="${remote.skill}"]`).dispatchEvent('pointerover',
    { clientX: 600, clientY: 400 });
  await page.locator('body').click({ position: { x: 2, y: 2 } });
  await page.keyboard.press('s');
  await expect.poll(async () => (await saved(page)).attributeChoices[String(remote.skill)]).toBe('str');
  const after = await capture(page);
  expect(after.state.allocatedNodes.length).toBe(route.length);
  expect(allocationAccess(graph, new Set(after.state.allocatedNodes), witchRoot).connected.size).toBe(route.length);
  expect(after.state.attributeChoices).toEqual({ [String(remote.skill)]: 'str' });
});

test('disconnected imported islands remain preserved and cannot receive a new allocation', async ({ page }) => {
  await seeded(page, { allocatedNodes: [3419], attributeChoices: {}, jewels: [] });
  const original = await savedBytes(page);
  const details = await keyboardResult(page, 'Attribute', 722);
  await expect(details.getByRole('button', { name: 'Allocate', exact: true })).toBeDisabled();
  await expect(page.locator('.tree-edit-preview')).toContainText('disconnected');
  await pointerNode(page, 722);
  await expect(page.getByRole('alert')).toContainText('disconnected');
  expect(await savedBytes(page)).toEqual(original);
});

test('keyboard socket inspection is read-only and Refund matches pointer jewel dependency cleanup in both weapon sets', async ({ page, browser }) => {
  const radii = JSON.parse(readFileSync(new URL(`../../data/${version}/base/jewel_radii.json`, import.meta.url), 'utf8'));
  const bands = radii.tree_versions[Object.keys(radii.tree_versions)
    .sort((a, b) => a.localeCompare(b, undefined, { numeric: true })).at(-1)!];
  const band = bands[catalog.ring_sizes['only affects passives in medium ring'] - 1];
  const path = shortestAllocationPath(graph, new Set(), witchRoot, socket.skill);
  const dependents = nodes.filter(node => {
    const distance = Math.hypot(node.x! - socket.x!, node.y! - socket.y!);
    return node.kind === 'notable' && !node.ascendancy_id && !node.unlock_constraint
      && !path.includes(node.skill) && distance >= band.inner * radii.distance_multiplier
      && distance <= band.outer * radii.distance_multiplier;
  });
  const [dependent, inactiveDependent] = dependents;
  const ringText = 'Rarity: UNIQUE\nControlled Metamorphosis\nDiamond\nRadius: Variable\nImplicits: 0\nOnly affects Passives in Medium Ring\nPassives in Radius can be Allocated without being connected to your tree';
  const patch: Partial<SavedSession['state']> = {
    allocatedNodes: [...path, dependent.skill],
    jewels: [{ socket_node: socket.skill, text: ringText }],
    weaponSwap: { active: 1, alternate_items: [], exclusive_nodes: [[dependent.skill], [inactiveDependent.skill]] },
  };
  await seeded(page, patch);
  const original = await capture(page);
  const context = await newControlContext(browser);
  try {
    const pointer = await context.newPage();
    await seeded(pointer, patch);
    await pointerNode(pointer, socket.skill);
    await pointer.getByRole('button', { name: 'Unallocate socket', exact: true }).click();
    await expect.poll(async () => (await saved(pointer)).jewels).toEqual([]);
    const expected = await capture(pointer);
    expect(expected.state.allocatedNodes).not.toContain(socket.skill);
    expect(expected.state.allocatedNodes).not.toContain(dependent.skill);
    expect(expected.state.weaponSwap?.exclusive_nodes).toEqual([[], []]);

    const details = await keyboardResult(page, 'Jewel Socket', socket.skill);
    expect(await capture(page)).toEqual(original);
    await details.getByRole('button', { name: 'Edit jewel', exact: true }).focus();
    await page.keyboard.press('Enter');
    await expect(page.locator('.jewel-editor')).toBeVisible();
    expect(await capture(page)).toEqual(original);
    await page.getByRole('button', { name: 'Cancel', exact: true }).click();
    expect(await capture(page)).toEqual(original);
    await details.getByRole('button', { name: 'Refund', exact: true }).focus();
    await page.keyboard.press('Enter');
    await expect.poll(async () => (await saved(page)).jewels).toEqual([]);
    expect(await capture(page)).toEqual(expected);
  } finally {
    await context.close();
  }
});

test('selected conquered keystone details refresh to the real original text after jewel removal without stale translations', async ({ page }) => {
  const radii = JSON.parse(readFileSync(new URL(`../../data/${version}/base/jewel_radii.json`, import.meta.url), 'utf8'));
  const bands = radii.tree_versions[Object.keys(radii.tree_versions)
    .sort((a, b) => a.localeCompare(b, undefined, { numeric: true })).at(-1)!];
  const radius = bands.find((band: { label: string }) => band.label === 'Very Large').outer * radii.distance_multiplier;
  const sockets = nodes.filter(node => node.kind === 'jewel_socket' && node.id.startsWith('jewel_slot'));
  const pair = sockets.flatMap(socket => nodes.filter(node => node.kind === 'keystone' && !node.ascendancy_id
    && Math.hypot(node.x! - socket.x!, node.y! - socket.y!) <= radius).map(node => ({ socket, node })))[0];
  const path = shortestAllocationPath(graph, new Set(), witchRoot, pair.socket.skill);
  await seeded(page, {
    allocatedNodes: path,
    jewels: [{ socket_node: pair.socket.skill, text: 'Rarity: UNIQUE\nHeroic Tragedy\nTimeless Jewel\nRadius: Very Large\nImplicits: 0\nRemembrancing 1234 songworthy deeds by the line of Vorana\nPassives in radius are Conquered by the Kalguur\nHistoric' }],
  });
  const original = await capture(page);
  const replacement = original.result.tree_effects!.nodes[String(pair.node.skill)];
  expect(replacement.name).toBe('Black Scythe Training');
  let details = await keyboardResult(page, replacement.name, pair.node.skill);
  await expect(details.getByRole('heading')).toHaveText(replacement.name);
  await expect(details).toContainText('Gain no inherent bonus from Strength');
  expect(await capture(page)).toEqual(original);
  await language(page, 'zh-CN');
  details = page.getByRole('group', { name: bindT('zh-CN')('tree.nodeDetails'), exact: true });
  const translated = JSON.parse((await replayBackend()).translateLinesToZhCn(JSON.stringify([replacement.name])));
  await expect(details.getByRole('heading')).toHaveText(translated[0]);
  await pointerNode(page, pair.socket.skill);
  await page.getByRole('button', { name: bindT('zh-CN')('items.remove'), exact: true }).click();
  await expect.poll(async () => (await saved(page)).jewels).toEqual([]);
  await language(page, 'en-US');
  details = page.getByRole('group', { name: 'Node details', exact: true });
  await expect(details.getByRole('heading')).toHaveText(pair.node.name!);
  const raw = (pair.node.stats ?? []).map(line => line
    .replace(/\[([^\]|]*)\|([^\]]*)\]/g, '$2').replace(/\[([^\]]*)\]/g, '$1'));
  await expect(details.locator('.tree-node-details-stats > li')).toHaveText(raw);
  expect((await saved(page)).allocatedNodes).toEqual(original.state.allocatedNodes);
});
