import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import type { PassiveNode } from '../src/api/types';
import { buildPassiveGraph } from '../src/lib/passiveGraph';
import { choose, goTo, settled } from './buildLibrary';
import { importBuild } from './skillsAndCalcs';

const code = readFileSync(new URL('../../examples/poe-ninja/2.txt', import.meta.url), 'utf8').trim();
const version = readFileSync(new URL('../../data/CURRENT', import.meta.url), 'utf8').trim();
const nodes: PassiveNode[] = JSON.parse(readFileSync(new URL(`../../data/${version}/base/passive_tree.json`, import.meta.url), 'utf8'));
const graph = buildPassiveGraph(nodes);

test('refunding an allocated leaf marks the computed heat map stale', async ({ page }) => {
  await importBuild(page, code);
  await goTo(page, 'Tree');
  await expect.poll(() => page.locator('.tree-nodes circle').count(), { timeout: 30_000 }).toBeGreaterThan(1000);

  // Acceleration is a non-socket leaf in this committed build, connected only to #3431.
  // Prove the fixture precondition before refunding it once; no probing arbitrary nodes.
  const allocated: number[] = await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state.allocatedNodes);
  expect(allocated).toContain(43082);
  expect(graph.get(43082)?.filter(skill => allocated.includes(skill))).toEqual([3431]);
  const leaf = page.locator('circle[data-skill-id="43082"]');
  await expect(leaf).toHaveClass(/node-allocated/);
  await page.getByRole('searchbox', { name: 'Search nodes…', exact: true }).fill('Acceleration');
  await expect(page.locator('circle.node-search-hit')).toHaveCount(1);
  await page.locator('.tree-search').getByRole('button', { name: 'Next', exact: true }).click();

  const heat = page.locator('.tree-heat');
  await choose(page, heat.getByRole('combobox'), 'Life');
  const compute = heat.getByRole('button', { name: 'Compute heat map', exact: true });
  await compute.click();
  await expect.poll(() => page.locator('.tree-nodes circle[style*="fill"]').count(), { timeout: 60_000 }).toBeGreaterThan(5);
  await expect(compute).toBeEnabled({ timeout: 60_000 });
  await settled(page);

  const count = page.locator('.tree-count');
  const before = await count.innerText();
  await leaf.click();
  await expect(leaf).not.toHaveClass(/node-allocated/, { timeout: 30_000 });
  await expect(count).not.toHaveText(before);
  await expect(heat.locator('.tree-hint')).toHaveText('stale — recompute', { timeout: 30_000 });
  await expect(heat.getByRole('progressbar')).toHaveCount(0);
  await expect(compute).toBeEnabled();
});
