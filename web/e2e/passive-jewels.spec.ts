import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import type { PassiveNode, WeaponSwap } from '../src/api/types';
import { buildPassiveGraph, shortestAllocationPath } from '../src/lib/passiveGraph';
import { statValue } from './shell';

const version = readFileSync('../data/CURRENT', 'utf8').trim();
const read = (name: string) => JSON.parse(readFileSync(`../data/${version}/${name}.json`, 'utf8'));
const nodes: PassiveNode[] = read('base/passive_tree');
const catalog = read('overlay/passive_jewels');
const radii = read('base/jewel_radii');
const starts = new Set<string>(Object.values(catalog.class_starts));
const root = nodes.find(node => node.id === catalog.class_starts.witch)!.skill;
const graph = buildPassiveGraph(nodes.filter(node => !node.ascendancy_id && (!starts.has(node.id) || node.skill === root)));
const sockets = nodes.filter(node => node.kind === 'jewel_socket' && node.id.startsWith('jewel_slot'));
const path = (socket: PassiveNode) => shortestAllocationPath(graph, new Set(), root, socket.skill);
const socket = sockets.sort((a, b) => path(a).length - path(b).length)[0];
const allocated = path(socket);
const bands = radii.tree_versions[Object.keys(radii.tree_versions).sort((a, b) => a.localeCompare(b, undefined, { numeric: true })).at(-1)!];
const band = bands[catalog.ring_sizes['only affects passives in medium ring'] - 1];
const distance = (a: PassiveNode, b: PassiveNode) => Math.hypot(a.x! - b.x!, a.y! - b.y!);
const target = nodes.find(node => node.kind === 'notable' && !node.ascendancy_id && !node.unlock_constraint
  && !allocated.includes(node.skill) && distance(node, socket) >= band.inner * radii.distance_multiplier
  && distance(node, socket) <= band.outer * radii.distance_multiplier)!;
const ringText = 'Rarity: UNIQUE\nControlled Metamorphosis\nDiamond\nVersion: Old\nVersion: Current\nVariant: Small\nVariant: Medium\nSelected Variant: 2\nRadius: Variable\nImplicits: 0\n{variant:1}Only affects Passives in Small Ring\n{variant:2}Only affects Passives in Medium Ring\nPassives in Radius can be Allocated without being connected to your tree\n{range:0.5}-(20-10)% to all Elemental Resistances';
const saved = (page: Page) => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state);
const circle = (page: Page, id: number) => page.locator(`circle[data-skill-id="${id}"]`);
const editor = (page: Page) => page.locator('.jewel-editor');
const draftText = (page: Page) => editor(page).getByRole('textbox', { name: 'Jewel socket', exact: true });
async function chooseSocket(page: Page, id: number) {
  await page.getByRole('combobox', { name: 'Choose an allocated jewel socket' }).click();
  await page.locator(`[role="option"][data-value="${id}"]`).click();
}

async function chooseUnique(page: Page, name: string) {
  await editor(page).getByRole('button', { name: 'Choose a unique jewel' }).click();
  const catalog = editor(page).getByRole('region', { name: 'Unique catalogue' });
  await catalog.getByRole('searchbox').fill(name);
  await catalog.locator('.unique-result').filter({ hasText: name }).click();
  return catalog;
}

async function holdNextWorkerCall(page: Page, method: string) {
  return page.evaluateHandle(method => {
    const post = Worker.prototype.postMessage;
    const pending = { held: false, finish: (_fail: boolean) => {} };
    Worker.prototype.postMessage = function (message, options) {
      if (message?.method !== method) return Reflect.apply(post, this, [message, options]);
      Worker.prototype.postMessage = post;
      pending.held = true;
      pending.finish = fail => {
        if (fail) this.dispatchEvent(new MessageEvent('message', { data: {
          id: message.id, kind: 'error', name: 'Error', message: 'jewel test rejection',
        } }));
        else Reflect.apply(post, this, [message, options]);
      };
    };
    return pending;
  }, method);
}

async function start(page: Page, text: string, socketNode = socket, allocation = allocated, weaponSwap?: WeaponSwap, socketGroups: unknown[] = []) {
  await page.addInitScript(({ text, socket, allocated, weaponSwap, socketGroups }) => {
    localStorage.setItem('pobr-v2-build-state', JSON.stringify({ version: 1, notes: '', state: {
      pobCode: null, character: { level: 90, class_name: 'Witch', ascendancy_name: '' },
      allocatedNodes: allocated, attributeChoices: {}, socketGroups, items: [], flasks: [], weaponSwap,
      annotations: {}, params: { config_inputs: {} }, jewels: [{ socket_node: socket, text }],
    } }));
  }, { text, socket: socketNode.skill, allocated: allocation, weaponSwap, socketGroups });
  await page.goto('/tree');
  await expect(circle(page, socketNode.skill)).toHaveClass(/node-jewel-filled/, { timeout: 90_000 });
  await expect(page.locator('.tree-asc-picker select').first()).toBeEnabled();
}

test('selected ring grants one-point allocation and removal revokes only the dependent point', async ({ page }) => {
  expect(target).toBeDefined();
  await start(page, ringText);
  await expect(circle(page, target.skill)).toHaveClass(/node-jewel-allocatable/);
  await circle(page, socket.skill).dispatchEvent('click');
  await expect(page.locator(`.jewel-radius[data-jewel-source="${socket.skill}"]`)).toHaveCount(1);
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await circle(page, target.skill).dispatchEvent('click');
  await expect.poll(async () => (await saved(page)).allocatedNodes.length).toBe(allocated.length + 1);
  await expect(circle(page, target.skill)).toHaveClass(/node-allocated/);
  await expect(page.locator('.tree-asc-picker select').first()).toBeEnabled();
  await circle(page, socket.skill).dispatchEvent('click');
  await page.getByRole('button', { name: 'Remove', exact: true }).click();
  await expect.poll(async () => (await saved(page)).jewels.length).toBe(0);
  // The saved list is kept in ascending order; the path is in walking order.
  expect((await saved(page)).allocatedNodes).toEqual([...allocated].sort((a, b) => a - b));
  await expect(circle(page, target.skill)).not.toHaveClass(/node-allocated|node-jewel-allocatable/);
  await expect(page.locator('.shell-alert')).toHaveCount(0);
});

test('a catalogued Adorned scales a corrupted jewel in the other socket until replaced', async ({ page }) => {
  const other = sockets.find(node => node.skill !== socket.skill && path(node).length > 0)!;
  const magic = 'Rarity: MAGIC\nRuby\nImplicits: 0\n+100 to maximum Life\n+10 to maximum Mana\nCorrupted';
  await start(page, magic, socket, [...new Set([...allocated, ...path(other)])]);
  const life = async () => Number((await statValue(page, /^Life$/).innerText()).replaceAll(',', ''));
  const baseline = await life();
  expect(baseline).toBeGreaterThan(100);
  const toggle = page.getByRole('button', { name: 'Attributes & planning' });
  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');
  await circle(page, other.skill).dispatchEvent('click');
  await expect(draftText(page)).toBeVisible();
  await expect(draftText(page)).toHaveValue('');
  const catalog = await chooseUnique(page, 'The Adorned');
  await catalog.getByRole('button', { name: 'All maximum rolls' }).click();
  await catalog.getByRole('button', { name: 'Use in socket draft' }).click();
  await expect(draftText(page)).toHaveValue(/150% increased Effect/);
  expect((await saved(page)).jewels).toEqual([{ socket_node: socket.skill, text: magic }]);
  await editor(page).getByRole('button', { name: 'Apply changes', exact: true }).click();
  await expect.poll(life).toBeGreaterThan(baseline);
  await chooseSocket(page, other.skill);
  const rare = 'Rarity: RARE\nBright Spark\nEmerald\nImplicits: 0\n+8 to maximum Mana\n+6% to Fire Resistance';
  await draftText(page).fill(rare);
  await editor(page).getByRole('button', { name: 'Apply changes', exact: true }).click();
  await expect.poll(life).toBe(baseline);
  expect((await saved(page)).jewels).toEqual(expect.arrayContaining([
    { socket_node: socket.skill, text: magic }, { socket_node: other.skill, text: rare },
  ]));
});

test('crafts a rare jewel with two affixes in the socket and can discard the draft', async ({ page }) => {
  const original = 'Rarity: MAGIC\nRuby\nImplicits: 0\n+20 to maximum Life';
  await start(page, original);
  await chooseSocket(page, socket.skill);
  await editor(page).getByRole('button', { name: 'Craft a jewel' }).click();
  const craft = editor(page).getByRole('region', { name: 'Craft item' });
  await craft.getByRole('button', { name: 'Create draft' }).click();
  const affixes = craft.locator('.replacement-affix-editor');
  for (const kind of ['Prefixes', 'Suffixes']) {
    await affixes.getByRole('button', { name: kind, exact: true }).click();
    await affixes.getByRole('combobox', { name: 'Choose a modifier and tier' }).click();
    await page.getByRole('option').first().click();
    await affixes.locator('.affix-editor-add > .trade-primary').click();
  }
  await expect(affixes.locator('.affix-editor-row')).toHaveCount(2);
  await affixes.getByRole('button', { name: 'Use in socket draft' }).click();
  await expect(draftText(page)).toHaveValue(/Rarity: RARE/);
  const crafted = await draftText(page).inputValue();
  expect(crafted.match(/^LevelReq: \d+$/gm)).toHaveLength(1);
  expect(crafted.split('Implicits: 0\n').at(-1)!.trim().split('\n').length).toBeGreaterThanOrEqual(2);
  await editor(page).getByRole('button', { name: 'Cancel', exact: true }).click();
  await chooseSocket(page, socket.skill);
  await expect(draftText(page)).toHaveValue(original);
  await draftText(page).fill(crafted);
  await editor(page).getByRole('button', { name: 'Apply changes', exact: true }).click();
  await expect.poll(async () => (await saved(page)).jewels).toEqual([{ socket_node: socket.skill, text: crafted }]);
});

test('a rejected ring replacement keeps the editable draft and dependent point until retry', async ({ page }) => {
  await start(page, ringText, socket, [...allocated, target.skill]);
  await chooseSocket(page, socket.skill);
  const original = await saved(page);
  const replacement = 'Rarity: MAGIC\nRuby\nImplicits: 0\n+20 to maximum Life';
  await draftText(page).fill(replacement);
  const pending = await holdNextWorkerCall(page, 'loadPassiveTree');
  const apply = editor(page).getByRole('button', { name: 'Apply changes' });
  await apply.click();
  await expect.poll(() => pending.evaluate(call => call.held)).toBe(true);
  await expect(editor(page).getByRole('button', { name: 'Cancel', exact: true })).toBeDisabled();
  await pending.evaluate(call => call.finish(true));
  await expect(apply).toBeEnabled();
  await expect(draftText(page)).toHaveValue(replacement);
  expect(await saved(page)).toEqual(original);
  await apply.click();
  await expect(editor(page)).toHaveCount(0);
  await expect.poll(async () => (await saved(page)).jewels[0].text).toBe(replacement);
  expect((await saved(page)).allocatedNodes).not.toContain(target.skill);
});

test('a late catalogue response preserves newer text and library changes can be cancelled', async ({ page }) => {
  await start(page, ringText);
  await chooseSocket(page, socket.skill);
  const original = await saved(page);
  const catalog = await chooseUnique(page, 'The Adorned');
  await expect(catalog.locator('.unique-roll-value')).toContainText('increased Effect');
  const pending = await holdNextWorkerCall(page, 'uniqueItemDraft');
  await catalog.getByRole('button', { name: 'Use in socket draft' }).click();
  await expect.poll(() => pending.evaluate(call => call.held)).toBe(true);
  const replacement = 'Rarity: RARE\nSaved Spark\nRuby\nImplicits: 0\n+20 to maximum Life';
  await draftText(page).fill(replacement);
  await pending.evaluate(call => call.finish(false));
  await expect(catalog.getByRole('button', { name: 'Use in socket draft' })).toBeEnabled();
  await expect(draftText(page)).toHaveValue(replacement);
  await editor(page).getByRole('button', { name: 'Save to item library' }).click();
  await editor(page).getByRole('button', { name: 'Cancel', exact: true }).click();
  await chooseSocket(page, socket.skill);
  const switcher = editor(page).getByRole('combobox', { name: 'Switch item' });
  await switcher.click();
  await page.getByRole('option', { name: /Saved Spark/ }).click();
  await expect(draftText(page)).toHaveValue(replacement);
  expect(await saved(page)).toEqual(original);
  await switcher.click();
  await page.getByRole('option', { name: '(unequip)' }).click();
  await expect(draftText(page)).toHaveValue('');
  await editor(page).getByRole('button', { name: 'Cancel', exact: true }).click();
  expect(await saved(page)).toEqual(original);
});

test('replaces a jewel by its Chinese name from the narrow tree toolbar and confirms the equipped jewel', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.addInitScript(() => localStorage.setItem('pobr-lang', 'zh-CN'));
  await start(page, 'Rarity: MAGIC\nRuby\nImplicits: 0\n+20 to maximum Life');
  const picker = page.getByRole('combobox', { name: '选择已分配的珠宝孔' });
  const counter = page.locator('.tree-count');
  await expect(counter).toContainText(/已用 \d+ \/ \d+ 点/);
  const originalPoints = await counter.innerText();
  await expect(picker).toBeInViewport({ ratio: 1 });
  await picker.click();
  await page.locator(`[role="option"][data-value="${socket.skill}"]`).click();
  const apply = editor(page).getByRole('button', { name: '应用修改' });
  await expect(apply).toBeDisabled();
  await editor(page).getByRole('button', { name: '选择传奇珠宝' }).click();
  await editor(page).getByRole('searchbox').fill('浮华');
  await editor(page).getByRole('button', { name: /浮华/ }).click();
  await editor(page).getByRole('button', { name: '填入珠宝草稿' }).click();
  await expect(editor(page).locator('textarea')).toHaveValue(/The Adorned/);
  await expect(apply).toBeFocused();
  await apply.press('Enter');
  const details = page.locator(`.tree-node-details[data-skill-id="${socket.skill}"]`);
  await expect(details).toBeFocused();
  await expect(details.getByRole('status').filter({ hasText: '浮华' })).toBeInViewport();
  await expect(details.getByRole('button', { name: '退点', exact: true })).toBeEnabled();
  await expect(counter).toHaveText(originalPoints);
  expect(new Set((await saved(page)).allocatedNodes)).toEqual(new Set(allocated));
});

test('replacing a catalogue ring with a radius-grant jewel changes the overlay and revokes remote allocation', async ({ page }) => {
  await start(page, 'Rarity: MAGIC\nRuby\nImplicits: 0\n+1 to maximum Life');
  await chooseSocket(page, socket.skill);
  const catalog = await chooseUnique(page, 'Controlled Metamorphosis');
  await catalog.getByRole('combobox', { name: 'Variant', exact: true }).click();
  await page.getByRole('option', { name: 'Medium Ring', exact: true }).click();
  await catalog.getByRole('button', { name: 'Use in socket draft' }).click();
  await editor(page).getByRole('button', { name: 'Apply changes', exact: true }).click();
  const overlay = page.locator(`.jewel-radius[data-jewel-source="${socket.skill}"]`);
  await expect(overlay).toBeVisible();
  await expect(circle(page, target.skill)).toHaveClass(/node-jewel-allocatable/);
  await circle(page, target.skill).dispatchEvent('click');
  await expect(circle(page, target.skill)).toHaveClass(/node-allocated/);
  await chooseSocket(page, socket.skill);
  const beforeReplacement = await saved(page);
  await draftText(page).fill('Rarity: RARE\nTime Shard\nTime-Lost Ruby\nRadius: Small\nImplicits: 0\nSmall Passive Skills in Radius also grant +2 to Strength');
  await editor(page).getByRole('button', { name: 'Apply changes', exact: true }).click();
  await expect(circle(page, target.skill)).not.toHaveClass(/node-allocated|node-jewel-allocatable/);
  await expect(overlay).toHaveAttribute('d', /^M[^M]+$/);
  expect((await saved(page)).allocatedNodes).not.toContain(target.skill);
  await page.getByRole('button', { name: 'Undo tree edit', exact: true }).click();
  await expect.poll(() => saved(page)).toEqual(beforeReplacement);
  await expect(circle(page, target.skill)).toHaveClass(/node-allocated/);
  await expect(overlay).toHaveAttribute('d', /^M.+M/);
});

test('alternate start is implicit and replacing its jewel revokes its remote path', async ({ page }) => {
  const other = nodes.find(node => node.id === catalog.class_starts.ranger)!;
  const allGraph = buildPassiveGraph(nodes);
  const next = nodes.find(node => allGraph.get(other.skill)?.includes(node.skill)
    && node.kind === 'normal' && node.name !== 'Attribute' && !allocated.includes(node.skill))!;
  await start(page, "Rarity: UNIQUE\nSplit Personality\nRuby\nImplicits: 0\nCan Allocate Passive Skills from the Ranger's starting point");
  await circle(page, next.skill).dispatchEvent('click');
  await expect.poll(async () => (await saved(page)).allocatedNodes.length).toBe(allocated.length + 1);
  expect((await saved(page)).allocatedNodes).not.toContain(other.skill);
  await expect(page.locator('.tree-asc-picker select').first()).toBeEnabled();
  await circle(page, socket.skill).dispatchEvent('click');
  await page.getByRole('textbox', { name: 'Jewel socket', exact: true }).fill('Rarity: MAGIC\nRuby\nImplicits: 0\n+20 to maximum Life');
  await page.getByRole('button', { name: 'Apply changes', exact: true }).click();
  // The saved list is kept in ascending order; the path is in walking order.
  await expect.poll(async () => (await saved(page)).allocatedNodes).toEqual([...allocated].sort((a, b) => a - b));
  await expect(page.locator('.shell-alert')).toHaveCount(0);
});

test('conquered keystone shows its replacement and warns about unavailable seed data', async ({ page }) => {
  const radius = bands.find((value: { label: string }) => value.label === 'Very Large').outer * radii.distance_multiplier;
  const pair = sockets.flatMap(socket => nodes.filter(node => node.kind === 'keystone' && !node.ascendancy_id
    && distance(node, socket) <= radius).map(node => ({ socket, node })))[0];
  expect(pair).toBeDefined();
  await start(page, 'Rarity: UNIQUE\nHeroic Tragedy\nTimeless Jewel\nRadius: Very Large\nImplicits: 0\nRemembrancing 1234 songworthy deeds by the line of Vorana\nPassives in radius are Conquered by the Kalguur\nHistoric', pair.socket, path(pair.socket));
  await expect(circle(page, pair.node.skill)).toHaveClass(/node-transformed/);
  await circle(page, pair.node.skill).dispatchEvent('pointerover', { clientX: 600, clientY: 400 });
  await expect(page.locator('.tree-tooltip')).toContainText('Black Scythe Training');
  await expect(page.locator('.tree-tooltip')).toContainText('Gain no inherent bonus from Strength');
  await expect(page.getByText('Known jewel transformations are shown.', { exact: false })).toBeVisible();
});

test('removing an allocation jewel also cleans its inactive weapon-set dependency', async ({ page }) => {
  await start(page, ringText, socket, allocated, { active: 1, alternate_items: [], exclusive_nodes: [[], [target.skill]] });
  expect((await saved(page)).weaponSwap.exclusive_nodes[1]).toEqual([target.skill]);
  await circle(page, socket.skill).dispatchEvent('click');
  await page.getByRole('button', { name: 'Remove', exact: true }).click();
  await expect.poll(async () => (await saved(page)).jewels.length).toBe(0);
  expect((await saved(page)).weaponSwap.exclusive_nodes).toEqual([[], []]);
  expect((await saved(page)).allocatedNodes).toEqual(allocated);
});

test('joint planning pays for an empty socket and applies the exact jewel and passive preview', async ({ page }) => {
  const initialNodes = allocated.filter(id => id !== socket.skill);
  await page.addInitScript(({ allocated }) => {
    localStorage.setItem('pobr-v2-build-state', JSON.stringify({ version: 1, notes: '', state: {
      pobCode: null, character: { level: 90, class_name: 'Witch', ascendancy_name: '' },
      allocatedNodes: allocated, attributeChoices: {}, socketGroups: [], items: [], flasks: [], jewels: [],
      annotations: {}, params: { config_inputs: {} },
    } }));
  }, { allocated: initialNodes });
  await page.goto('/tree');
  await expect(page.locator('.tree-asc-picker select').first()).toBeEnabled({ timeout: 90_000 });
  await page.getByRole('button', { name: 'Plan passive upgrades' }).click();
  await page.getByRole('combobox', { name: 'Objective', exact: true }).click();
  await page.getByRole('option', { name: 'Max Life', exact: true }).click();
  await page.getByRole('spinbutton', { name: 'Point budget', exact: true }).fill('2');
  const planner = page.getByRole('region', { name: 'Jewels and passive routes' });
  const candidate = `${ringText}\n+100 to maximum Life`;
  await planner.getByRole('textbox', { name: 'Candidate jewel text' }).fill(candidate);
  await planner.getByRole('combobox', { name: 'Candidate socket', exact: true }).click();
  await page.locator(`[role="option"][data-value="${socket.skill}"]`).click();
  await planner.getByRole('button', { name: 'Add jewel placement', exact: true }).click();
  const original = await saved(page);
  await planner.getByRole('button', { name: 'Find jewel and passive plans', exact: true }).click();
  const first = planner.locator('.tree-planner-results > li').first();
  await expect(planner.locator('.jewel-plan-results')).toBeVisible({ timeout: 90_000 });
  await expect(first).toBeVisible();
  await expect(first).toContainText('Controlled Metamorphosis');
  expect((await saved(page))).toEqual(original);
  const predictedLife = Number((await first.locator('output[data-stat="Life"]').innerText()).replaceAll(',', ''));
  await first.getByRole('button', { name: 'Show joint route on tree', exact: true }).click();
  await expect(circle(page, socket.skill)).toHaveClass(/node-planned-add/);
  await first.getByRole('button', { name: 'Apply jewel and passive plan', exact: true }).click();
  await expect.poll(async () => (await saved(page)).jewels.length).toBe(1);
  await expect.poll(async () => Number((await statValue(page, /^Life$/).innerText()).replaceAll(',', '')))
    .toBe(Math.round(predictedLife));
  const after = await saved(page);
  expect(after.allocatedNodes).toContain(socket.skill);
  expect(after.allocatedNodes.length - original.allocatedNodes.length).toBeLessThanOrEqual(2);
  expect(after.jewels[0]).toEqual({ socket_node: socket.skill, text: candidate });
  expect(after.items).toEqual(original.items);
  expect(after.socketGroups).toEqual(original.socketGroups);
  expect(after.params).toEqual(original.params);
  await expect(planner.locator('.jewel-plan-results')).toHaveCount(0);
  await expect(page.locator('.shell-alert')).toHaveCount(0);
});

test('joint refund plan replaces an allocation jewel and removes its dependent point atomically', async ({ page }) => {
  await start(page, ringText, socket, [...allocated, target.skill]);
  await page.getByRole('button', { name: 'Plan passive upgrades' }).click();
  await page.getByRole('combobox', { name: 'Objective', exact: true }).click();
  await page.getByRole('option', { name: 'Max Life', exact: true }).click();
  await page.getByRole('combobox', { name: 'Plan', exact: true }).click();
  await page.getByRole('option', { name: 'Reallocate existing points', exact: true }).click();
  await page.getByRole('spinbutton', { name: 'Maximum refunds', exact: true }).fill('1');
  const planner = page.getByRole('region', { name: 'Jewels and passive routes' });
  const replacement = 'Rarity: MAGIC\nRuby\nImplicits: 0\n+100 to maximum Life';
  await planner.getByRole('textbox', { name: 'Candidate jewel text' }).fill(replacement);
  await planner.getByRole('button', { name: 'Add jewel placement', exact: true }).click();
  const original = await saved(page);
  await planner.getByRole('button', { name: 'Find jewel and passive plans', exact: true }).click();
  await expect(planner.locator('.jewel-plan-results')).toBeVisible({ timeout: 90_000 });
  const first = planner.locator('.tree-planner-results > li').first();
  await expect(first.locator('.tree-plan-heading')).toContainText('Ruby');
  await expect(first.locator('.tree-plan-heading')).toContainText('Refund 1');
  const predictedLife = Number((await first.locator('output[data-stat="Life"]').innerText()).replaceAll(',', ''));
  await first.getByRole('button', { name: 'Apply jewel and passive plan', exact: true }).click();
  await expect.poll(async () => (await saved(page)).jewels[0].text).toBe(replacement);
  await expect.poll(async () => Number((await statValue(page, /^Life$/).innerText()).replaceAll(',', '')))
    .toBe(Math.round(predictedLife));
  const after = await saved(page);
  expect(after.allocatedNodes).not.toContain(target.skill);
  expect(after.allocatedNodes.length).toBeLessThanOrEqual(original.allocatedNodes.length);
  expect(after.allocatedNodes).toContain(socket.skill);
  await expect(page.locator('.shell-alert')).toHaveCount(0);
});

test('joint planner rejects unmodeled jewels and invalidates results after budget edits', async ({ page }) => {
  await start(page, 'Rarity: MAGIC\nRuby\nImplicits: 0\n+1 to maximum Life');
  await page.getByRole('button', { name: 'Plan passive upgrades' }).click();
  await page.getByRole('combobox', { name: 'Objective', exact: true }).click();
  await page.getByRole('option', { name: 'Max Life', exact: true }).click();
  await page.getByRole('spinbutton', { name: 'Point budget', exact: true }).fill('1');
  const planner = page.getByRole('region', { name: 'Jewels and passive routes' });
  await planner.getByRole('textbox', { name: 'Candidate jewel text' }).fill('Rarity: MAGIC\nRuby\nImplicits: 0\n+100 to maximum Life\nThis jewel grants an unmodeled power');
  await planner.getByRole('button', { name: 'Add jewel placement', exact: true }).click();
  const original = await saved(page);
  await planner.getByRole('button', { name: 'Find jewel and passive plans', exact: true }).click();
  await expect(planner.locator('.jewel-plan-results')).toBeVisible({ timeout: 90_000 });
  await expect(planner.locator('.jewel-plan-results')).toContainText('Excluded or unmodeled candidates');
  await planner.getByText('Excluded or unmodeled candidates', { exact: true }).click();
  await expect(planner.locator('.jewel-plan-results')).toContainText('unmodeled power');
  await expect(planner.locator('.tree-plan-heading').filter({ hasText: 'Ruby' })).toHaveCount(0);
  expect(await saved(page)).toEqual(original);
  await page.getByRole('spinbutton', { name: 'Point budget', exact: true }).fill('2');
  await expect(planner.locator('.jewel-plan-results')).toHaveCount(0);
});

test('Upgrades drops a retained jewel position when its socket is unallocated on Tree', async ({ page }) => {
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  // Scoring a jewel position needs a skill to measure against.
  await start(page, 'Rarity: MAGIC\nRuby\nImplicits: 0\n+20 to maximum Life', socket, allocated, undefined,
    [{ enabled: true, gems: [{ skill_id: 'FireballPlayer', level: 16, quality: 0 }] }]);
  const nav = (name: string) => page.getByRole('navigation', { name: 'Main navigation' }).getByRole('link', { name, exact: true });
  const position = page.locator(`.trade-position[data-slot-id="Jewel@${socket.skill}"]`);
  await nav('Upgrades').click();
  await position.click();
  await expect(position).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByRole('button', { name: 'Calculate modifier scores', exact: true })).toBeEnabled();

  await nav('Tree').click();
  await circle(page, socket.skill).dispatchEvent('click');
  await page.getByRole('button', { name: 'Unallocate socket', exact: true }).click();
  await expect(circle(page, socket.skill)).not.toHaveClass(/node-allocated/);
  expect((await saved(page)).allocatedNodes).not.toContain(socket.skill);

  await nav('Upgrades').click();
  await expect(position).toHaveCount(0);
  await expect(page.locator('.trade-position[aria-pressed="true"] strong')).toHaveText('Main Hand');
});
