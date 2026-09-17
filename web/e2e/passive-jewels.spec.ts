import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import type { PassiveNode, WeaponSwap } from '../src/api/types';
import { buildPassiveGraph, shortestAllocationPath } from '../src/lib/passiveGraph';

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
const saved = (page: Page) => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!).state);
const circle = (page: Page, id: number) => page.locator(`circle[data-skill-id="${id}"]`);

async function start(page: Page, text: string, socketNode = socket, allocation = allocated, weaponSwap?: WeaponSwap) {
  await page.addInitScript(({ text, socket, allocated, weaponSwap }) => {
    localStorage.setItem('pobr-tab', 'tree');
    localStorage.setItem('pobr-build-state', JSON.stringify({ version: 1, notes: '', state: {
      pobCode: null, character: { level: 90, class_name: 'Witch', ascendancy_name: '' },
      allocatedNodes: allocated, attributeChoices: {}, socketGroups: [], items: [], flasks: [], weaponSwap,
      annotations: {}, params: { config_inputs: {} }, jewels: [{ socket_node: socket, text }],
    } }));
  }, { text, socket: socketNode.skill, allocated: allocation, weaponSwap });
  await page.goto('/');
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
  expect((await saved(page)).allocatedNodes).toEqual(allocated);
  await expect(circle(page, target.skill)).not.toHaveClass(/node-allocated|node-jewel-allocatable/);
  await expect(page.locator('.calc-error')).toHaveCount(0);
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
  await page.getByRole('button', { name: 'Save & recalculate', exact: true }).click();
  await expect.poll(async () => (await saved(page)).allocatedNodes).toEqual(allocated);
  await expect(page.locator('.calc-error')).toHaveCount(0);
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
