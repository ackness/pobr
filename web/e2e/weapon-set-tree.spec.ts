import { expect, test, type Page } from '@playwright/test';
import { busy, openItemsAfterImport, pageLink } from './shell';
import { choose, generateShareCode } from './treePage';
import { readFileSync } from 'node:fs';
import { deflateSync } from 'node:zlib';
import type { PassiveNode } from '../src/api/types';
import type { SavedSession } from '../src/hooks/useBuildSession';

const version = readFileSync(new URL('../../data/CURRENT', import.meta.url), 'utf8').trim();
const nodes: PassiveNode[] = JSON.parse(readFileSync(new URL(`../../data/${version}/base/passive_tree.json`, import.meta.url), 'utf8'));
const catalog = JSON.parse(readFileSync(new URL(`../../data/${version}/overlay/passive_jewels.json`, import.meta.url), 'utf8'));
const root = nodes.find(node => node.id === catalog.class_starts.warrior)!;
const adjacent = nodes.filter(node => !node.ascendancy_id && node.kind === 'normal'
  && (root.connections?.includes(node.skill) || node.connections?.includes(root.skill))).sort((a,b) => a.skill - b.skill);
const one = adjacent[0];
const two = adjacent[1];
const child = nodes.filter(node => node.kind === 'normal' && node.name !== 'Attribute'
  && node.skill !== root.skill && !adjacent.some(other => other.skill === node.skill)
  && !node.ascendancy_id && !node.unlock_constraint
  && (one.connections?.includes(node.skill) || node.connections?.includes(one.skill)))
  .sort((a,b) => a.skill - b.skill)[0];
const firstSet = [one.skill, child.skill].sort((a,b) => a - b);
const code = deflateSync(`<PathOfBuilding2><Build level="80" className="Warrior"/><Tree activeSpec="1"><Spec nodes="" treeVersion="0_${version.split('.')[1]}"/></Tree><Skills/><Items/><Notes>Weapon tree round-trip</Notes></PathOfBuilding2>`).toString('base64url');
const nav = (page: Page, name: string) => pageLink(page, name);
const saved = (page: Page): Promise<SavedSession['state']> => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state);
const circle = (page: Page, node: PassiveNode) => page.locator(`circle[data-skill-id="${node.skill}"]`);
async function idle(page: Page) {
  await expect(busy(page)).toHaveCount(0);
  await expect(page.locator('.shell-save-state')).toContainText('Saved locally');
  await expect(page.locator('.shell-boot[role=alert], .shell-alert')).toHaveCount(0);
}
async function importCode(page: Page, source: string) {
  await page.locator('.shell-builds-button').click();
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(source);
  await page.locator('.import-submit').click();
  await openItemsAfterImport(page);
  await expect(page.getByRole('heading', { name: 'Equipment', exact: true })).toBeVisible();
  await idle(page);
}
async function allocate(page: Page, node: PassiveNode) {
  await circle(page, node).dispatchEvent('click', { clientX: 600, clientY: 400 });
  if (node.name === 'Attribute') await page.getByRole('menuitem', { name: /^Strength/ }).click();
  await expect.poll(async () => (await saved(page)).allocatedNodes.includes(node.skill)).toBe(true);
  await idle(page);
}
async function inspect(page: Page, node: PassiveNode) {
  await page.getByRole('searchbox', { name: 'Search nodes…', exact: true }).fill(node.name!);
  const details = page.getByRole('group', { name: 'Node details', exact: true });
  const hits = await page.locator('circle.node-search-hit').count();
  for (let i = 0; i < hits; i++) {
    await page.getByRole('button', { name: 'Next', exact: true }).click();
    if (await details.getAttribute('data-skill-id') === String(node.skill)) return details;
  }
  throw new Error(`Missing node ${node.skill}`);
}

test('set allocation, previews, promotion and export preserve the inactive tree', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1100 });
  // Only unrelated league discovery is recorded; the tree, edits, export and
  // calculations all run the shipped WASM and current game data.
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.goto('/build');
  await expect(page.getByRole('textbox', { name: 'Build code', exact: true })).toBeVisible({ timeout: 90_000 });
  await importCode(page, code);
  await nav(page, 'Tree').click();
  const mode = page.getByRole('combobox', { name: 'Allocate to', exact: true });
  await choose(page, mode, 'Weapon set 2');
  await idle(page);
  await circle(page, two).dispatchEvent('pointerover', { clientX: 600, clientY: 400 });
  await expect(circle(page, two)).toHaveClass(/node-preview-add/);
  await allocate(page, two);
  await expect(circle(page, two)).toHaveAttribute('data-weapon-set', '2');
  await expect(circle(page, two)).toHaveClass(/node-preview-refund/);
  await choose(page, mode, 'Weapon set 1');
  await idle(page);
  await expect(circle(page, two)).not.toHaveClass(/node-allocated/);
  expect((await saved(page)).weaponSwap?.exclusive_nodes).toEqual([[], [two.skill]]);
  await circle(page, child).dispatchEvent('pointerover', { clientX: 600, clientY: 400 });
  await expect(circle(page, one)).toHaveClass(/node-preview-add/);
  await expect(circle(page, child)).toHaveClass(/node-preview-add/);
  await allocate(page, child);
  const exported = await generateShareCode(page);
  await importCode(page, exported);
  const roundtrip = await saved(page);
  expect(roundtrip.weaponSwap?.active).toBe(1);
  expect(roundtrip.weaponSwap?.exclusive_nodes).toEqual([firstSet, [two.skill]]);
  expect(roundtrip.allocatedNodes).toEqual(firstSet);
  await nav(page, 'Tree').click();
  await choose(page, mode, 'Weapon set 2');
  await idle(page);
  await expect(circle(page, two)).toHaveClass(/node-allocated/);
  await expect(circle(page, one)).not.toHaveClass(/node-allocated/);
  await page.reload();
  await expect(mode).toBeVisible({ timeout: 90_000 });
  await idle(page);
  expect((await saved(page)).weaponSwap?.exclusive_nodes).toEqual([firstSet, [two.skill]]);
  const details = await inspect(page, two);
  await details.getByRole('button', { name: 'Make shared', exact: true }).click();
  await expect.poll(async () => (await saved(page)).weaponSwap?.exclusive_nodes).toEqual([firstSet, []]);
  await expect(circle(page, two)).toHaveAttribute('data-weapon-set', '0');
  await choose(page, mode, 'Weapon set 1');
  await idle(page);
  await expect(circle(page, two)).toHaveClass(/node-allocated/);
  await inspect(page, one);
  await page.locator('.tree-bar-top').scrollIntoViewIfNeeded();
  await circle(page, one).dispatchEvent('pointerover', { clientX: 600, clientY: 400 });
  await expect(circle(page, one)).toHaveClass(/node-preview-refund/);
  await expect(circle(page, child)).toHaveClass(/node-preview-refund/);
  await page.screenshot({ path: test.info().outputPath('weapon-set-tree.png') });
  await circle(page, one).dispatchEvent('click', { clientX: 600, clientY: 400 });
  await expect.poll(async () => (await saved(page)).weaponSwap?.exclusive_nodes).toEqual([[], []]);
  expect((await saved(page)).allocatedNodes).toEqual([two.skill]);
});
