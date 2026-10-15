import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import type { PassiveNode } from '../src/api/types';
import { comparisonWorkspace } from './fixtures/compare-workspace';
import { workspaceEnvelope, type BuildWorkspace } from '../src/lib/buildWorkspace';

const version = readFileSync(new URL('../../data/CURRENT', import.meta.url), 'utf8').trim();
const nodes: PassiveNode[] = JSON.parse(readFileSync(new URL(`../../data/${version}/base/passive_tree.json`, import.meta.url), 'utf8'));
const oldNodes: PassiveNode[] = JSON.parse(readFileSync(new URL(`../../data/${version}/base/passive_trees/0_4.json`, import.meta.url), 'utf8'));
const root = nodes.find(node => node.name === 'RANGER')!;
const distance = (a: PassiveNode, b: PassiveNode) => Math.hypot(a.x! - b.x!, a.y! - b.y!);
const sockets = nodes.filter(node => node.kind === 'jewel_socket' && !node.ascendancy_id && node.x !== undefined)
  .sort((a, b) => distance(a, root) - distance(b, root));
const socket = sockets[0], inactiveSocket = sockets[1];
const attrs = nodes.filter(node => node.name === 'Attribute' && !node.ascendancy_id && node.x !== undefined)
  .sort((a, b) => distance(a, socket) - distance(b, socket));
const jewel = (name: string, modifier: string) => `Rarity: RARE\n${name}\nEmerald\nImplicits: 0\n${modifier}`;
const snapshot = (page: Page) => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!).workspace);

async function openComparison(page: Page, workspace: BuildWorkspace) {
  await page.addInitScript(raw => { if (!localStorage.getItem('pobr-build-state')) localStorage.setItem('pobr-build-state', raw); }, workspaceEnvelope(workspace));
  await page.goto('/');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  const original = await snapshot(page);
  await page.getByRole('button', { name: 'Compare', exact: true }).click();
  await page.getByRole('combobox', { name: 'Build / stage A', exact: true }).selectOption('stage-a');
  await page.getByRole('combobox', { name: 'Build / stage B', exact: true }).selectOption('stage-b');
  return original;
}

test('overlay both allocations, inspect socket jewels and attributes, pan and zoom without changing either stage', async ({ page }, testInfo) => {
  const workspace = comparisonWorkspace();
  const [a, b] = workspace.builds[0].stages.map(stage => stage.saved.state);
  a.allocatedNodes = [root.skill, socket.skill, attrs[0].skill];
  b.allocatedNodes = [root.skill, socket.skill, attrs[1].skill];
  a.attributeChoices = { [attrs[0].skill]: 'dex' };
  b.attributeChoices = { [attrs[1].skill]: 'str' };
  a.jewels = [{ socket_node: socket.skill, text: jewel('Starter Jewel', '+5 to Dexterity') }];
  b.jewels = [{ socket_node: socket.skill, text: jewel('Target Jewel', '+8 to Strength') },
    { socket_node: inactiveSocket.skill, text: jewel('Spare Jewel', '+20 to Intelligence') }];
  const original = await openComparison(page, workspace);
  const panel = page.getByRole('region', { name: 'Tree comparison', exact: true });
  const canvas = panel.locator('.compare-tree-canvas');
  await expect(canvas).toBeVisible();
  await expect(panel.locator('.compare-tree-legend [data-side="a"]')).toHaveText('● Only A (1)');
  await expect(panel.locator('.compare-tree-legend [data-side="b"]')).toHaveText('■ Only B (1)');
  await expect(panel.locator('.compare-tree-legend [data-side="both"]')).toHaveText('◎ Both (2)');
  await expect(canvas.locator(`[data-skill-id="${attrs[0].skill}"]`)).toHaveAttribute('data-side', 'a');
  await expect(canvas.locator(`[data-skill-id="${attrs[1].skill}"]`)).toHaveAttribute('data-side', 'b');
  const shared = canvas.locator(`[data-skill-id="${socket.skill}"]`);
  await expect(shared).toHaveAttribute('data-side', 'both');
  await expect(shared.locator('[data-jewel]')).toHaveCount(2);
  await expect(panel.locator('[data-total="passive"] td')).toHaveText(['2', '2']);
  await expect(panel.locator('[data-total="ascendancy"] td')).toHaveText(['0', '0']);
  await expect(panel.locator('[data-total="choice-dex"] td')).toHaveText(['1', '0']);
  await expect(panel.locator('[data-total="choice-str"] td')).toHaveText(['0', '1']);
  await expect(panel.locator('[data-total="Str"] td').first()).toHaveText(/^\d+$/);
  await page.getByRole('checkbox', { name: 'Only changed stats', exact: true }).uncheck();
  for (const id of ['Str', 'Dex', 'Int']) {
    const summary = page.locator(`.compare-summary [data-stat="${id}"] td`);
    await expect(panel.locator(`[data-total="${id}"] td`)).toHaveText(await summary.allTextContents().then(values => values.slice(0, 2)));
  }
  await expect(panel.locator(`[data-socket="${socket.skill}"]`)).toContainText('Starter Jewel');
  await expect(panel.locator(`[data-socket="${socket.skill}"]`)).toContainText('Target Jewel');
  await expect(panel.locator(`[data-socket="${inactiveSocket.skill}"]`)).toContainText('Unallocated socket (inactive jewel)');
  // Selecting a socket via keyboard and pointer is inspection, never allocation.
  await panel.getByRole('combobox', { name: 'Inspect passive', exact: true }).selectOption(String(socket.skill));
  await shared.focus();
  await page.keyboard.press('Enter');
  const details = panel.getByRole('group', { name: 'Node details', exact: true });
  await expect(details).toContainText('Starter Jewel');
  await expect(details).toContainText('Target Jewel');
  await canvas.locator(`[data-skill-id="${attrs[0].skill}"]`).click();
  await expect(panel.getByRole('combobox', { name: 'Inspect passive', exact: true })).toHaveValue(String(attrs[0].skill));
  await expect(details).toContainText('Attribute choice: Dexterity');
  const previous = await canvas.getAttribute('viewBox');
  await panel.getByRole('button', { name: 'Zoom in', exact: true }).click();
  expect(await canvas.getAttribute('viewBox')).not.toBe(previous);
  const zoomed = await canvas.getAttribute('viewBox');
  await canvas.focus();
  await page.keyboard.press('ArrowRight');
  expect(await canvas.getAttribute('viewBox')).not.toBe(zoomed);
  const bounds = (await canvas.boundingBox())!;
  const panned = await canvas.getAttribute('viewBox');
  await page.mouse.move(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2);
  await page.mouse.down();
  await page.mouse.move(bounds.x + bounds.width / 2 + 60, bounds.y + bounds.height / 2 + 40, { steps: 4 });
  await page.mouse.up();
  expect(await canvas.getAttribute('viewBox')).not.toBe(panned);
  expect(await snapshot(page)).toEqual(original);
  await panel.getByRole('button', { name: 'Fit allocations', exact: true }).click();
  await page.setViewportSize({ width: 1440, height: 1100 });
  await panel.getByRole('heading', { name: 'Tree comparison', exact: true }).evaluate(element => element.scrollIntoView({ block: 'start' }));
  await page.screenshot({ path: testInfo.outputPath('tree-comparison.png') });
  await panel.locator('.compare-tree-totals').scrollIntoViewIfNeeded();
  await page.screenshot({ path: testInfo.outputPath('tree-totals.png') });
  await page.setViewportSize({ width: 640, height: 900 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.reload();
  await expect(page.locator('.compare-tree-canvas')).toBeVisible({ timeout: 90_000 });
  expect(await snapshot(page)).toEqual(original);
});

test('version mismatch keeps IDs distinct and warns when a historical allocation has no coordinates', async ({ page }) => {
  const common = oldNodes.find(node => node.x === undefined && nodes.some(current => current.skill === node.skill && current.x !== undefined && !current.ascendancy_id))!;
  const workspace = comparisonWorkspace();
  const [a, b] = workspace.builds[0].stages.map(stage => stage.saved.state);
  a.allocatedNodes = [common.skill];
  b.allocatedNodes = [common.skill];
  b.treeVersion = '0_4';
  const original = await openComparison(page, workspace);
  const panel = page.getByRole('region', { name: 'Tree comparison', exact: true });
  await expect(panel).toContainText('A · 0_5 / B · 0_4');
  await expect(panel).toContainText('Tree versions differ. Matching numeric IDs are not marked as shared');
  await expect(panel.locator('.compare-tree-legend [data-side="both"]')).toHaveText('◎ Both (0)');
  await expect(panel.locator('.compare-tree-warning')).toContainText(['Tree versions differ', 'B · Allocated nodes without positions in this version (1)']);
  await expect(panel.locator(`[data-overlay-key="a:${common.skill}"]`)).toHaveAttribute('data-side', 'a');
  await expect(panel.locator(`[data-overlay-key="b:${common.skill}"]`)).toHaveCount(0);
  await panel.getByRole('combobox', { name: 'Inspect passive', exact: true }).selectOption(`b:${common.skill}`);
  await expect(panel.getByRole('group', { name: 'Node details', exact: true })).toContainText(common.name!);
  expect(await snapshot(page)).toEqual(original);
});
