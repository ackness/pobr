/**
 * Data-bump acceptance: the ordinary player flow on the shipped WASM and the data the
 * lock currently pins. Import a real build code, see its skills and supports, open the
 * item crafter, allocate a passive, then reload and find the same build again.
 *
 * CI runs this spec whenever `pipeline/data.lock.json` changes (ci.yml, step
 * "Data-bump smoke"); locally it needs what every spec needs: `pnpm build-wasm`,
 * `pnpm sync-data` and a build (the preview server serves dist/).
 *
 * It checks that each step works and that the data resolves (every imported gem is in
 * the gem catalog, the crafter lists bases and affixes, a node next to the build's tree
 * can be allocated), not particular numbers: a new data release should pass without edits
 * unless something a player uses broke. Numerical agreement is the parity suites' job.
 */

import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import type { PassiveNode } from '../src/api/types';
import { buildPassiveGraph } from '../src/lib/passiveGraph';
import { busy, statValue } from './shell';
import { goTo, headlineDps, importBuild } from './skillsAndCalcs';
import { stableText } from './buildLibrary';

const code = readFileSync(new URL('../../examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt', import.meta.url), 'utf8').trim();
const version = readFileSync(new URL('../../data/CURRENT', import.meta.url), 'utf8').trim();
const nodes: PassiveNode[] = JSON.parse(readFileSync(new URL(`../../data/${version}/base/passive_tree.json`, import.meta.url), 'utf8'));

const savedState = (page: Page) => page.evaluate(() =>
  JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state as { allocatedNodes: number[] });

/** No calculation running, no error banner, the save indicator healthy and the results current. */
async function settled(page: Page) {
  await expect(busy(page)).toHaveCount(0, { timeout: 90_000 });
  await expect(page.locator('.shell-boot[role=alert], .shell-alert')).toHaveCount(0);
  await expect(page.locator('.shell-save-state')).not.toHaveClass(/is-error/);
  await expect(page.locator('#character-stats[data-stale]')).toHaveCount(0);
}

const treeGraph = buildPassiveGraph(nodes.filter(node => !node.ascendancy_id));
const ordinary = (node: PassiveNode) => node.kind === 'normal' && !node.ascendancy_id && node.name !== 'Attribute';

/** An allocated ordinary passive at the end of a branch: refunding it disconnects nothing else. */
function leafNode(allocated: number[]): PassiveNode {
  const taken = new Set(allocated);
  const leaf = nodes.find(node => ordinary(node) && taken.has(node.skill)
    && (treeGraph.get(node.skill) ?? []).filter(other => taken.has(other)).length === 1);
  if (!leaf) throw new Error('The imported tree has no allocated normal passive at the end of a branch');
  return leaf;
}

/** An unallocated ordinary passive next to the allocated tree. */
function nextNode(allocated: number[], except: number): PassiveNode {
  const taken = new Set(allocated);
  const next = nodes.find(node => ordinary(node) && node.skill !== except && !taken.has(node.skill)
    && (treeGraph.get(node.skill) ?? []).some(other => taken.has(other)));
  if (!next) throw new Error('The imported tree has no unallocated normal passive next to it');
  return next;
}

/** Existing tree specs dispatch on the SVG node's own handler rather than hit-testing pixels. */
async function clickNode(page: Page, skill: number) {
  const rect = await page.locator('.tree-canvas svg').boundingBox();
  await page.locator(`circle[data-skill-id="${skill}"]`).dispatchEvent('click', {
    clientX: rect!.x + Math.min(150, rect!.width / 2), clientY: rect!.y + Math.min(150, rect!.height / 2),
  });
}

test('after a data bump: import, skills and supports, crafting, passives, save and reopen', async ({ page }) => {
  // The results panel eases numbers toward new values; reduced motion shows them settled.
  await page.emulateMedia({ reducedMotion: 'reduce' });

  // 1. Import a real build code: it calculates and lands on the Overview with results.
  await importBuild(page, code);
  await settled(page);
  // This build is Chaos Inoculation (Life is always 1), so follow numbers that move with the
  // tree and the data: Energy Shield and the headline DPS.
  const energyShield = statValue(page, /^Energy Shield$/).first();
  const dps = headlineDps(page);
  for (const value of [energyShield, dps]) {
    await expect(value).not.toHaveText('—');
    await expect(value).not.toHaveText('0');
  }

  // 2. Skills: one main group whose gems all resolve in the gem catalog, supports included.
  await goTo(page, 'Skills');
  const main = page.locator('.skill-group.is-main');
  await expect(main).toHaveCount(1);
  await expect(main.locator('.skill-group-supports')).not.toBeEmpty();
  await main.locator('.skill-group-title').click();
  const gems = page.locator('.skill-gems .skill-gem');
  await expect(gems.first()).toBeVisible();
  await expect(page.locator('.skill-gems .skill-gem[data-gem-kind="active"]').first()).toBeVisible();
  expect(await page.locator('.skill-gems .skill-gem[data-gem-kind="support"]').count()).toBeGreaterThan(0);
  await expect(page.locator('.skill-gems .skill-gem:not([data-gem-kind])')).toHaveCount(0);
  const supportsBefore = await main.locator('.skill-group-supports').innerText();

  // 3. Items: the equipment shows, and the crafter lists game bases and their affixes.
  await goTo(page, 'Items');
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 30_000 });
  await page.getByRole('button', { name: 'Craft item', exact: true }).click();
  const crafter = page.getByRole('region', { name: 'Craft item', exact: true });
  await crafter.getByRole('combobox', { name: 'Item base', exact: true }).click();
  await page.getByRole('option').first().click();
  await expect(crafter.locator('.item-crafter-base')).not.toBeEmpty();
  await crafter.getByRole('button', { name: 'Create draft', exact: true }).click();
  const editor = crafter.locator('.replacement-affix-editor');
  await expect(editor).toHaveAttribute('open', '');
  await editor.getByRole('combobox', { name: 'Choose an affix and tier', exact: true }).click();
  expect(await page.getByRole('option').count()).toBeGreaterThan(0);
  await page.keyboard.press('Escape');
  await settled(page);

  // 4. Tree: the imported build spends every point, so refund a leaf passive, then allocate
  // another next to the tree. Both edits are saved and recalculated.
  await goTo(page, 'Tree');
  await expect(page.locator('.tree-canvas svg')).toBeVisible({ timeout: 30_000 });
  const imported = (await savedState(page)).allocatedNodes;
  const sorted = (nodes: number[]) => [...nodes].sort((a, b) => a - b);
  const leaf = leafNode(imported);
  await expect(page.locator(`circle[data-skill-id="${leaf.skill}"]`)).toHaveClass(/node-allocated/);
  await clickNode(page, leaf.skill);
  await expect(page.locator(`circle[data-skill-id="${leaf.skill}"]`)).not.toHaveClass(/node-allocated/);
  // Exactly that one node goes: a cascade refund (or a tree that lost its class start) fails here.
  const refunded = sorted(imported.filter(node => node !== leaf.skill));
  await expect.poll(async () => sorted((await savedState(page)).allocatedNodes)).toEqual(refunded);
  await settled(page);
  const target = nextNode(refunded, leaf.skill);
  const circle = page.locator(`circle[data-skill-id="${target.skill}"]`);
  await expect(circle).not.toHaveClass(/node-allocated/);
  await clickNode(page, target.skill);
  await expect(circle).toHaveClass(/node-allocated/);
  const edited = sorted([...refunded, target.skill]);
  await expect.poll(async () => sorted((await savedState(page)).allocatedNodes)).toEqual(edited);
  await settled(page);
  const energyShieldAfterEdit = await stableText(energyShield);
  const dpsAfterEdit = await stableText(dps);

  // 5. Save and reopen: the autosaved build comes back with the edit, its skills and its results.
  await page.reload();
  await expect(page.locator('.tree-canvas svg')).toBeVisible({ timeout: 90_000 });
  await settled(page);
  await expect(page.locator(`circle[data-skill-id="${target.skill}"]`)).toHaveClass(/node-allocated/);
  await expect(page.locator(`circle[data-skill-id="${leaf.skill}"]`)).not.toHaveClass(/node-allocated/);
  expect(sorted((await savedState(page)).allocatedNodes)).toEqual(edited);
  await expect(energyShield).toHaveText(energyShieldAfterEdit!, { timeout: 30_000 });
  await expect(dps).toHaveText(dpsAfterEdit!, { timeout: 30_000 });
  await goTo(page, 'Skills');
  await expect(page.locator('.skill-group.is-main .skill-group-supports')).toHaveText(supportsBefore);
});
