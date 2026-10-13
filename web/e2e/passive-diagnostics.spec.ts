import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import type { PassiveNode } from '../src/api/types';
import { buildPassiveGraph, shortestAllocationPath } from '../src/lib/passiveGraph';

const version = readFileSync('../data/CURRENT', 'utf8').trim();
const nodes: PassiveNode[] = JSON.parse(readFileSync(`../data/${version}/base/passive_tree.json`, 'utf8'));
const catalog = JSON.parse(readFileSync(`../data/${version}/overlay/passive_jewels.json`, 'utf8'));
const starts = new Set<string>(Object.values(catalog.class_starts));
const root = nodes.find(node => node.id === catalog.class_starts.witch)!.skill;
const graph = buildPassiveGraph(nodes.filter(node => !node.ascendancy_id && (!starts.has(node.id) || node.skill === root)));
const socket = nodes.find(node => node.skill === 61419)!;
const allocated = shortestAllocationPath(graph, new Set(), root, socket.skill);
const attributeChoices = Object.fromEntries(nodes.filter(node => node.name === 'Attribute' && allocated.includes(node.skill))
  .map(node => [String(node.skill), 'int']));
// Pinned 4.5.5.3 topology: the allocated Vorana radius contains Attribute #46819;
// the two-point route to Elemental Damage #5936 traverses Attribute #49512.
// Kalguur seed-dependent transformations are deliberately unresolved, not scored as zero.
// jewel_tree.rs marks these radius nodes unresolved; calc_orchestrator/mod.rs preserves
// the exact Tree:<id>: missing timeless jewel seed data engine diagnostic.
const baselineReason = 'Tree:46819: missing timeless jewel seed data';
const newReasons = ['Tree:49512: missing timeless jewel seed data', 'Tree:5936: missing timeless jewel seed data'];
const timeless = 'Rarity: UNIQUE\nHeroic Tragedy\nTimeless Jewel\nRadius: Very Large\nImplicits: 0\nRemembrancing 1234 songworthy deeds by the line of Vorana\nPassives in radius are Conquered by the Kalguur\nHistoric';
const replacement = 'Rarity: MAGIC\nRuby\nImplicits: 0\n+20 to maximum Life';
const nav = (page: Page, name: string) => page.getByRole('navigation', { name: 'Main navigation' })
  .getByRole('button', { name, exact: true });
const baseline = (page: Page) => page.locator('.tree-planner-baseline-unsupported');
const exclusions = (page: Page) => page.locator('.tree-planner-excluded');
const run = (page: Page) => page.getByRole('button', { name: 'Find upgrade routes', exact: true });
const saved = (page: Page) => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!).state);

async function syntheticTimelessWitch(page: Page) {
  // Synthetic persisted build/league fixture only. Jewel validation, radius membership,
  // baseline calculation, complete route batches and diagnostics use real shipped WASM.
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.addInitScript(({ allocated, attributeChoices, socket, timeless }) => {
    localStorage.setItem('pobr-tab', 'tree');
    localStorage.setItem('pobr-build-state', JSON.stringify({ version: 1, notes: '', state: {
      pobCode: null, character: { level: 90, class_name: 'Witch', ascendancy_name: '' },
      allocatedNodes: allocated, attributeChoices,
      socketGroups: [{ enabled: true, gems: [{ skill_id: 'FireballPlayer', level: 16, quality: 0 }] }],
      items: [], flasks: [], jewels: [{ socket_node: socket, text: timeless }],
      annotations: {}, params: { config_inputs: {} },
    } }));
  }, { allocated, attributeChoices, socket: socket.skill, timeless });
  await page.goto('/');
  await expect(page.locator(`circle[data-skill-id="${socket.skill}"]`)).toHaveClass(/node-jewel-filled/, { timeout: 90_000 });
  await expect(page.locator('.tree-asc-picker select').first()).toBeEnabled();
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
}

async function open(page: Page) {
  const toggle = page.getByRole('button', { name: 'Plan passive upgrades', exact: true });
  if (await toggle.getAttribute('aria-expanded') === 'false') await toggle.click();
  await expect(page.locator('.tree-planner-body')).toBeVisible();
}

async function configure(page: Page) {
  await open(page);
  await page.getByRole('button', { name: 'Objective', exact: true }).click();
  await page.getByRole('option', { name: 'Max Life', exact: true }).click();
  await page.getByRole('spinbutton', { name: 'Point budget', exact: true }).fill('2');
}

async function complete(page: Page) {
  await run(page).click();
  await expect(page.locator('.tree-planner-summary')).toBeVisible({ timeout: 90_000 });
  await expect(run(page)).toBeEnabled();
  await expect(page.locator('.opt-progress')).toHaveCount(0);
}

async function assertPairedDiagnostics(page: Page) {
  await expect(baseline(page)).toHaveCount(1);
  await baseline(page).locator('summary').click();
  await expect(baseline(page).locator('li').filter({ hasText: baselineReason })).toHaveText(baselineReason);
  for (const reason of newReasons) await expect(baseline(page)).not.toContainText(reason);
  const outer = exclusions(page).locator('..');
  await outer.locator(':scope > summary').click();
  // Match the complete allocation, not just a duplicated target name or reason elsewhere.
  const route = exclusions(page).locator(':scope > li').filter({ has: page.locator('p', {
    hasText: /^Allocate: Attribute #49512 · Elemental Damage #5936$/,
  }) });
  await expect(route).toHaveCount(1);
  await route.locator('summary').click();
  await expect(route.locator('ul > li')).toHaveText(newReasons);
  await expect(route).not.toContainText(baselineReason);
  await expect(route.getByRole('button', { name: 'Apply plan', exact: true })).toHaveCount(0);
  // Capture consumer-visible route/reason associations, not labels or incidental copy.
  return {
    baseline: await baseline(page).locator('li').allTextContents(),
    routes: await exclusions(page).locator(':scope > li').evaluateAll(cards => cards.map(card => ({
      allocate: card.querySelector('details > p')?.textContent,
      reasons: Array.from(card.querySelectorAll('ul > li')).map(reason => reason.textContent),
    }))),
  };
}

async function assertNoReceipt(page: Page) {
  await expect(page.locator('.tree-planner-summary')).toHaveCount(0);
  await expect(baseline(page)).toHaveCount(0);
  await expect(exclusions(page)).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Apply plan', exact: true })).toHaveCount(0);
  await expect(page.locator('.opt-progress')).toHaveCount(0);
  await expect(run(page)).toBeEnabled();
}

for (const edit of ['copied stage', 'jewel input'] as const) {
  test(`real-WASM passive diagnostics stay paired across navigation and invalidate on ${edit}`, async ({ page }) => {
    await syntheticTimelessWitch(page);
    await configure(page);
    await complete(page);
    const before = await saved(page);
    const receipt = await assertPairedDiagnostics(page);
    // First visits exercise the shared lazy boundary, not only warmed page navigation.
    await nav(page, 'Calcs').click();
    await expect(page.locator('.tree-canvas, .tree-planner-body')).toHaveCount(0);
    await nav(page, 'Skills').click();
    await expect(page.locator('.skills-toolbar input')).toBeEnabled();
    await nav(page, 'Upgrades').click();
    await expect(page.locator('.upgrade-paths')).toBeVisible();
    await nav(page, 'Tree').click();
    // Details may remain open through navigation; close them before asserting again.
    await baseline(page).evaluate(element => { (element as HTMLDetailsElement).open = false; });
    await exclusions(page).locator('..').evaluate(element => { (element as HTMLDetailsElement).open = false; });
    await exclusions(page).locator('details').evaluateAll(elements => elements.forEach(element => { (element as HTMLDetailsElement).open = false; }));
    expect(await assertPairedDiagnostics(page)).toEqual(receipt);
    expect(await saved(page)).toEqual(before);

    if (edit === 'copied stage') {
      await nav(page, 'Build').click();
      await page.getByRole('button', { name: 'Copy current stage', exact: true }).click();
      await page.getByRole('textbox', { name: 'Name', exact: true }).fill('Synthetic diagnostic copy');
      await page.getByRole('button', { name: 'Save', exact: true }).click();
      await expect(page.locator('.topbar-busy')).toHaveCount(0);
      await nav(page, 'Tree').click();
      await open(page);
      await assertNoReceipt(page);
      expect(await saved(page)).toEqual(before);
      // Identical build data still needs a fresh stage-owned diagnostic receipt.
      await configure(page);
      await complete(page);
      expect(await assertPairedDiagnostics(page)).toEqual(receipt);
    } else {
      // Navigation above stopped tracking, so editing the real jewel cannot silently
      // publish a fresh receipt or retain the old unresolved radius diagnostics.
      await page.locator(`circle[data-skill-id="${socket.skill}"]`).dispatchEvent('click');
      await page.getByRole('textbox', { name: 'Jewel socket', exact: true }).fill(replacement);
      await page.getByRole('button', { name: 'Save & recalculate', exact: true }).click();
      await expect.poll(async () => (await saved(page)).jewels).toEqual([{ socket_node: socket.skill, text: replacement }]);
      await expect(page.locator('.topbar-busy')).toHaveCount(0);
      await assertNoReceipt(page);
      expect((await saved(page)).allocatedNodes).toEqual(before.allocatedNodes);
      await complete(page);
      await expect(baseline(page)).toHaveCount(0);
      // A plain jewel leaves no radius node unresolved. A route through a node
      // with a line the engine does not parse stays excluded, for that line.
      const reasons = await exclusions(page).locator(':scope > li ul > li').allTextContents();
      for (const reason of reasons) expect(reason).toMatch(/^Tree:\d+: (?!missing timeless jewel seed data)/);
      await expect(page.locator('.tree-planner-body')).not.toContainText('missing timeless jewel seed data');
      await expect(page.locator('.calc-error')).toHaveCount(0);
    }
  });
}
