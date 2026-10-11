import { expect, test, type Locator, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { deflateSync } from 'node:zlib';
import type { AttributeChoice, JewelInput, PassiveNode, SlotItemInput, SocketGroupInput } from '../src/api/types';
import { buildPassiveGraph, shortestAllocationPath } from '../src/lib/passiveGraph';

const version = readFileSync('../data/CURRENT', 'utf8').trim();
const nodes: PassiveNode[] = JSON.parse(readFileSync(`../data/${version}/base/passive_tree.json`, 'utf8'));
const catalog = JSON.parse(readFileSync(`../data/${version}/overlay/passive_jewels.json`, 'utf8'));
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
const unmodeled = 'Rarity: MAGIC\nRuby\nImplicits: 0\n+100 to maximum Life\nThis jewel grants an unmodeled power';
const nav = (page: Page, name: string) => page.getByRole('navigation', { name: 'Main navigation' })
  .getByRole('button', { name, exact: true });
const routeResults = (page: Page) => page.locator('.tree-planner-body > .tree-planner-results');
const joint = (page: Page) => page.getByRole('region', { name: 'Jewels and passive routes' });
const routeRun = (page: Page) => page.getByRole('button', { name: 'Find upgrade routes', exact: true });
const budget = (page: Page) => page.getByRole('spinbutton', { name: 'Point budget', exact: true });
interface FixtureState {
  allocatedNodes: number[];
  attributeChoices: Record<string, AttributeChoice>;
  jewels: JewelInput[];
  items: SlotItemInput[];
  socketGroups: SocketGroupInput[];
  params: unknown;
  treeVersion?: string | null;
}
const saved = (page: Page) => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!).state as FixtureState);
const numeric = (text: string) => Number(text.replaceAll(',', ''));
const life = (page: Page) => page.locator('.stat-row').filter({ hasText: /^Life/ }).locator('dd');

type Planner = 'route' | 'joint';
const results = (page: Page, planner: Planner) => planner === 'route' ? routeResults(page) : joint(page).locator('.jewel-plan-results');
const run = (page: Page, planner: Planner) => planner === 'route' ? routeRun(page)
  : joint(page).getByRole('button', { name: 'Find jewel and passive plans', exact: true });

async function syntheticWitch(page: Page) {
  // Synthetic persisted build and league list only. Nodes, topology, jewel validation,
  // full-build calculations, candidate scoring and Apply all use real shipped WASM/data.
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.addInitScript(({ allocated, originalChoices, socket }) => {
    localStorage.setItem('pobr-tab', 'tree');
    localStorage.setItem('pobr-build-state', JSON.stringify({ version: 1, notes: '', state: {
      pobCode: null, character: { level: 90, class_name: 'Witch', ascendancy_name: '' },
      allocatedNodes: allocated, attributeChoices: originalChoices,
      socketGroups: [{ enabled: true, gems: [{ skill_id: 'FireballPlayer', level: 16, quality: 0 }] }],
      items: [{ slot: 'ring1', text: 'Rarity: RARE\nSynthetic Continuity Ring\nSapphire Ring\nImplicits: 0\n+80 to maximum Life' }],
      flasks: [], jewels: [{ socket_node: socket, text: 'Rarity: MAGIC\nRuby\nImplicits: 0\n+1 to maximum Life' }],
      annotations: {}, params: { config_inputs: {} },
    } }));
  }, { allocated, originalChoices, socket: socket.skill });
  await page.goto('/');
  await expect(page.locator(`circle[data-skill-id="${socket.skill}"]`)).toHaveClass(/node-jewel-filled/, { timeout: 90_000 });
  await expect(page.locator('.tree-asc-picker select').first()).toBeEnabled();
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
}

async function select(page: Page, label: string, option: string) {
  await page.getByRole('button', { name: label, exact: true }).click();
  await page.getByRole('option', { name: option, exact: true }).click();
}

async function open(page: Page) {
  await nav(page, 'Tree').click();
  const toggle = page.getByRole('button', { name: 'Plan passive upgrades', exact: true });
  if (await toggle.getAttribute('aria-expanded') === 'false') await toggle.click();
  await expect(page.locator('.tree-planner-body')).toBeVisible();
}

async function configure(page: Page) {
  await open(page);
  await select(page, 'Objective', 'Max Life');
  await budget(page).fill('3');
  await select(page, 'New travel attributes', 'Strength');
}

async function addPlacement(page: Page, text = jewel) {
  await joint(page).getByRole('textbox', { name: 'Candidate jewel text', exact: true }).fill(text);
  await joint(page).getByRole('button', { name: 'Candidate socket', exact: true }).click();
  await page.getByRole('option').filter({ hasText: `#${socket.skill}` }).click();
  await joint(page).getByRole('button', { name: 'Add jewel placement', exact: true }).click();
}

async function complete(page: Page, planner: Planner) {
  await run(page, planner).click();
  await expect(results(page, planner)).toBeVisible({ timeout: 90_000 });
  await expect(results(page, planner).locator('.tree-planner-results > li, :scope > li').first()).toBeVisible();
  await expect(run(page, planner)).toBeEnabled();
  await expect(page.locator('.opt-progress')).toHaveCount(0);
}

async function noReceipts(page: Page) {
  await expect(routeResults(page)).toHaveCount(0);
  await expect(joint(page).locator('.jewel-plan-results')).toHaveCount(0);
  await expect(page.getByRole('button', { name: /^Apply (plan|jewel and passive plan)$/ })).toHaveCount(0);
  await expect(page.locator('.opt-progress')).toHaveCount(0);
  await expect(routeRun(page)).toBeEnabled();
}

async function awayAndBack(page: Page) {
  // Calcs is deliberately the first lazy-page visit: a warmed boundary misses I2's loss.
  await nav(page, 'Calcs').click();
  await expect(page.locator('.tree-canvas')).toHaveCount(0);
  await expect(page.locator('.tree-planner-run')).toHaveCount(0);
  await nav(page, 'Skills').click();
  await expect(page.locator('.skills-toolbar input')).toBeEnabled();
  await nav(page, 'Upgrades').click();
  await expect(page.locator('.upgrade-paths')).toBeVisible();
  await nav(page, 'Tree').click();
  await expect(page.locator('.tree-planner-body')).toBeVisible();
}

async function previewNodes(page: Page, card: Locator, planner: Planner) {
  await card.getByRole('button', { name: planner === 'route' ? 'Show on tree' : 'Show joint route on tree', exact: true }).click();
  return {
    add: await page.locator('circle.node-planned-add[data-skill-id]').evaluateAll(elements => elements.map(e => Number(e.getAttribute('data-skill-id')))),
    remove: await page.locator('circle.node-planned-remove[data-skill-id]').evaluateAll(elements => elements.map(e => Number(e.getAttribute('data-skill-id')))),
  };
}

async function assertExactApply(page: Page, planner: Planner, card: Locator, before: FixtureState) {
  const preview = await previewNodes(page, card, planner);
  expect(preview.add.length, 'The selected preview must be an actual node edit').toBeGreaterThan(0);
  const expectedNodes = before.allocatedNodes.filter(id => !preview.remove.includes(id)).concat(preview.add);
  const choices: Record<string, AttributeChoice> = Object.fromEntries(Object.entries(before.attributeChoices)
    .filter(([id]) => expectedNodes.includes(Number(id))));
  for (const id of preview.add) if (nodes.find(node => node.skill === id)?.name === 'Attribute') choices[String(id)] = 'str';
  const predictedLife = numeric(await card.locator('output[data-stat="Life"]').innerText());
  const predictedDps = numeric(await card.locator('output[data-stat="TotalDPS"]').innerText());
  const predictedEhp = numeric(await card.locator('output[data-stat="TotalEHP"]').innerText());
  await card.getByRole('button', { name: planner === 'route' ? 'Apply plan' : 'Apply jewel and passive plan', exact: true }).click();
  await expect.poll(async () => [...(await saved(page)).allocatedNodes].sort((a, b) => a - b))
    .toEqual([...expectedNodes].sort((a, b) => a - b));
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  const after = await saved(page);
  expect(after.attributeChoices).toEqual(choices);
  expect(after.jewels).toEqual(planner === 'joint' ? [{ socket_node: socket.skill, text: jewel }] : before.jewels);
  expect(after.items).toEqual(before.items);
  expect(after.socketGroups).toEqual(before.socketGroups);
  expect(after.params).toEqual(before.params);
  await expect.poll(async () => numeric(await life(page).innerText())).toBe(Math.round(predictedLife));
  await expect.poll(async () => numeric(await page.locator('.stat-row').filter({ hasText: 'Total DPS' }).locator('dd').innerText())).toBe(predictedDps);
  await expect.poll(async () => numeric(await page.locator('.stat-row').filter({ hasText: 'Effective HP' }).locator('dd').innerText())).toBe(Math.round(predictedEhp));
  await expect(results(page, planner)).toHaveCount(0);
  await expect(page.locator('.calc-error')).toHaveCount(0);
}

test('route application confirmation follows the applied revision across refresh, navigation and edits', async ({ page }) => {
  await syntheticWitch(page);
  await configure(page);
  await complete(page, 'route');
  await awayAndBack(page);
  const before = await saved(page);
  await assertExactApply(page, 'route', routeResults(page).locator(':scope > li').first(), before);
  const confirmation = page.locator('.tree-planner-applied[role="status"]');
  await expect(confirmation).toBeVisible();
  const applied = await saved(page);
  for (const id of applied.allocatedNodes.filter(id => !before.allocatedNodes.includes(id))) {
    await expect(confirmation).toContainText(`#${id}`);
  }
  const receipt = await confirmation.innerText();
  await complete(page, 'route');
  await expect(confirmation).toHaveText(receipt, { useInnerText: true });
  await awayAndBack(page);
  await expect(confirmation).toHaveText(receipt, { useInnerText: true });

  // Restoring identical calculation inputs must not revive an older success message.
  for (const text of [jewel, applied.jewels[0].text]) {
    await page.locator(`circle[data-skill-id="${socket.skill}"]`).dispatchEvent('click');
    await page.getByRole('textbox', { name: 'Jewel socket', exact: true }).fill(text);
    await page.getByRole('button', { name: 'Save & recalculate', exact: true }).click();
    await expect.poll(async () => (await saved(page)).jewels[0].text).toBe(text);
    await expect(page.locator('.topbar-busy')).toHaveCount(0);
    await expect(confirmation).toHaveCount(0);
  }
  expect(await saved(page)).toEqual(applied);

  await complete(page, 'route');
  await awayAndBack(page);
  await assertExactApply(page, 'route', routeResults(page).locator(':scope > li').first(), await saved(page));
  await expect(confirmation).toBeVisible();
  await nav(page, 'Build').click();
  await page.getByRole('button', { name: 'Copy current stage', exact: true }).click();
  await page.getByRole('textbox', { name: 'Name', exact: true }).fill('Independent confirmation stage');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await nav(page, 'Tree').click();
  await open(page);
  await expect(confirmation).toHaveCount(0);
});

for (const planner of ['route', 'joint'] as const) {
  test(`${planner} completed receipts and settings survive first lazy navigation and Apply uses the exact full preview`, async ({ page }) => {
    await syntheticWitch(page);
    await configure(page);
    await addPlacement(page);
    await addPlacement(page, unmodeled);
    // Raw input is deliberately different from the added candidates.
    await joint(page).getByRole('textbox', { name: 'Candidate jewel text', exact: true }).fill(jewel);
    const candidates = await joint(page).locator('.jewel-plan-candidates').innerText();
    const socketLabel = await joint(page).getByRole('button', { name: 'Candidate socket', exact: true }).innerText();
    const before = await saved(page);
    await complete(page, planner);
    const receipt = await results(page, planner).innerText();
    if (planner === 'joint') await expect(results(page, planner)).toContainText('Excluded or unmodeled candidates');
    await awayAndBack(page);
    await expect(budget(page)).toHaveValue('3');
    await expect(page.getByRole('button', { name: 'Plan', exact: true })).toContainText('Spend new points');
    await expect(page.getByRole('button', { name: 'New travel attributes', exact: true })).toContainText('Strength');
    await expect(joint(page).getByRole('textbox', { name: 'Candidate jewel text', exact: true })).toHaveValue(jewel);
    await expect.poll(() => joint(page).getByRole('button', { name: 'Candidate socket', exact: true }).innerText()).toBe(socketLabel);
    await expect.poll(() => joint(page).locator('.jewel-plan-candidates').innerText()).toBe(candidates);
    await expect.poll(() => results(page, planner).innerText()).toBe(receipt);
    await expect(page.locator('.node-planned-add, .node-planned-remove, .opt-progress')).toHaveCount(0);
    expect(await saved(page)).toEqual(before);
    const cards = planner === 'route' ? routeResults(page).locator(':scope > li') : joint(page).locator('.tree-planner-results > li');
    let card = cards.first();
    if (planner === 'route') {
      // Require a consumer recommendation with travel attributes; don't allow a
      // no-attribute route to vacuously pass the evaluated-attribute regression.
      let found = false;
      for (let index = 0; index < await cards.count(); index++) {
        const candidate = cards.nth(index);
        const preview = await previewNodes(page, candidate, planner);
        if (preview.add.some(id => nodes.find(node => node.skill === id)?.name === 'Attribute')) {
          card = candidate; found = true; break;
        }
      }
      expect(found, 'Fixture must offer an improving route containing a new Strength travel node').toBe(true);
    } else {
      // Candidate text is visible in the complete route details, not a mocked response.
      card = cards.filter({ hasText: '+1000 to maximum Life' }).first();
    }
    await assertExactApply(page, planner, card, before);
  });
}

test('reallocation mode and maximum refunds survive navigation without changing the build', async ({ page }) => {
  await syntheticWitch(page);
  await configure(page);
  await select(page, 'Plan', 'Reallocate existing points');
  await page.getByRole('spinbutton', { name: 'Maximum refunds', exact: true }).fill('2');
  await addPlacement(page);
  const before = await saved(page);
  await routeRun(page).click();
  await expect(page.locator('.tree-planner-summary')).toBeVisible({ timeout: 90_000 });
  await expect(routeRun(page)).toBeEnabled();
  const receipt = await page.locator('.tree-planner-summary').innerText();
  await awayAndBack(page);
  await expect(page.getByRole('button', { name: 'Plan', exact: true })).toContainText('Reallocate existing points');
  await expect(page.getByRole('spinbutton', { name: 'Maximum refunds', exact: true })).toHaveValue('2');
  await expect(page.getByRole('button', { name: 'New travel attributes', exact: true })).toContainText('Strength');
  await expect(joint(page).locator('.jewel-plan-candidates > li')).toHaveCount(1);
  await expect.poll(() => page.locator('.tree-planner-summary').innerText()).toBe(receipt);
  expect(await saved(page)).toEqual(before);
});

for (const edit of ['equipment', 'goal'] as const) {
  test(`hidden ${edit} changes invalidate both completed planners without automatic restart or loss of settings`, async ({ page }) => {
    await syntheticWitch(page);
    await configure(page);
    await addPlacement(page);
    await complete(page, 'joint');
    // Route can complete alongside the joint receipt; starting joint cancels route tracking.
    await complete(page, 'route');
    const before = await saved(page);
    const beforeLife = await life(page).innerText();
    const floor = Math.max(0, numeric(await page.locator('.stat-row').filter({ hasText: 'Effective HP' }).locator('dd').innerText()) - 1);
    await nav(page, 'Upgrades').click();
    if (edit === 'equipment') {
      await page.locator('.trade-position').filter({ hasText: /^Ring 1/ }).click();
      await page.getByRole('textbox', { name: 'Complete item text', exact: true })
        .fill('Rarity: MAGIC\nSapphire Ring\nImplicits: 0\n+200 to maximum Life');
      await page.getByRole('button', { name: 'Calculate replacement', exact: true }).click();
      await expect(page.locator('.upgrade-replacement-result')).toBeVisible();
      await page.getByRole('button', { name: 'Apply to build · Ring 1', exact: true }).click();
      await expect(life(page)).not.toHaveText(beforeLife);
      await expect(page.locator('.topbar-busy')).toHaveCount(0);
    } else {
      await page.getByRole('button', { name: 'Max total DPS', exact: true }).click();
      await page.getByRole('checkbox', { name: 'Prioritize capped elemental resistances', exact: true }).check();
      await page.locator('.upgrade-goal-details > summary').click();
      await select(page, 'Constraint', 'Effective HP');
      await page.getByRole('spinbutton', { name: 'min', exact: true }).fill(String(floor));
      await expect(life(page)).toHaveText(beforeLife);
    }
    await expect(page.locator('.tree-canvas, .tree-planner-run')).toHaveCount(0);
    await page.clock.install();
    await page.clock.fastForward(2000);
    await nav(page, 'Tree').click();
    await noReceipts(page);
    await page.clock.fastForward(2000);
    await noReceipts(page);
    await expect(budget(page)).toHaveValue('3');
    await expect(page.getByRole('button', { name: 'New travel attributes', exact: true })).toContainText('Strength');
    await expect(joint(page).locator('.jewel-plan-candidates > li')).toHaveCount(1);
    const after = await saved(page);
    expect(after.allocatedNodes).toEqual(before.allocatedNodes);
    expect(after.attributeChoices).toEqual(before.attributeChoices);
    expect(after.jewels).toEqual(before.jewels);
    if (edit === 'goal') {
      await expect(page.getByRole('button', { name: 'Objective', exact: true })).toContainText('Max total DPS');
      await expect(page.getByRole('spinbutton', { name: 'min', exact: true })).toHaveValue(String(floor));
      await expect(page.getByRole('checkbox', { name: 'Prioritize capped elemental resistances', exact: true })).toBeChecked();
    }
    await page.clock.resume();
    await complete(page, 'route');
    expect((await saved(page)).allocatedNodes).toEqual(before.allocatedNodes);
  });
}

test('identical copied stages reset receipts and stage-local preferences without replaying prior planner focus', async ({ page }) => {
  await syntheticWitch(page);
  await open(page);
  const initialBudget = await budget(page).inputValue();
  const initialAttributes = await page.getByRole('button', { name: 'New travel attributes', exact: true }).innerText();
  await configure(page);
  await addPlacement(page);
  await complete(page, 'joint');
  await complete(page, 'route');
  const before = await saved(page);
  await nav(page, 'Build').click();
  const originalStage = await page.locator('.workspace-current-stage').innerText();
  await page.getByRole('button', { name: 'Copy current stage', exact: true }).click();
  await page.getByRole('textbox', { name: 'Name', exact: true }).fill('Synthetic passive copy');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await nav(page, 'Tree').click();
  await expect(page.getByRole('button', { name: 'Plan passive upgrades', exact: true })).toHaveAttribute('aria-expanded', 'false');
  await open(page);
  await noReceipts(page);
  await expect(budget(page)).toHaveValue(initialBudget);
  await expect.poll(() => page.getByRole('button', { name: 'New travel attributes', exact: true }).innerText()).toBe(initialAttributes);
  await expect(joint(page).getByRole('textbox', { name: 'Candidate jewel text', exact: true })).toHaveValue('');
  await expect(joint(page).locator('.jewel-plan-candidates > li')).toHaveCount(0);
  expect(await saved(page)).toEqual(before);
  await nav(page, 'Upgrades').click();
  await page.locator('.upgrade-paths > button').nth(2).click();
  await expect(page.locator('.tree-planner-body')).toBeVisible();
  await page.getByRole('combobox', { name: 'Stage', exact: true }).selectOption({ label: originalStage });
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Plan passive upgrades', exact: true })).toHaveAttribute('aria-expanded', 'false');
  await open(page);
  await noReceipts(page);
  await expect(budget(page)).toHaveValue(initialBudget);
  await expect(joint(page).locator('.jewel-plan-candidates > li')).toHaveCount(0);
  expect(await saved(page)).toEqual(before);
});

test('hidden tree has no canvas, hover, attribute editor or keyboard allocation ownership', async ({ page }) => {
  await syntheticWitch(page);
  await configure(page);
  await complete(page, 'route');
  const card = routeResults(page).locator(':scope > li').first();
  await previewNodes(page, card, 'route');
  const attribute = nodes.find(node => node.name === 'Attribute' && !allocated.includes(node.skill)
    && shortestAllocationPath(graph, new Set(allocated), root, node.skill).length === 1)!;
  expect(attribute, 'The keyboard fixture must open a directly reachable attribute picker').toBeDefined();
  await page.locator(`circle[data-skill-id="${attribute.skill}"]`).dispatchEvent('pointerover', { clientX: 600, clientY: 400 });
  await expect(page.locator('.tree-tooltip')).toBeVisible();
  await page.locator(`circle[data-skill-id="${attribute.skill}"]`).dispatchEvent('click');
  await expect(page.locator('.attr-picker')).toBeVisible();
  const before = await saved(page);
  // Do not move the pointer off the hovered node before navigation: a normal
  // mouse click can clear hover and accidentally conceal a retained key handler.
  await nav(page, 'Calcs').dispatchEvent('click');
  await expect(page.locator('.tree-canvas, .tree-tooltip, .attr-picker, .tree-asc-picker, .tree-planner-body')).toHaveCount(0);
  await page.locator('body').click({ position: { x: 2, y: 2 } });
  for (const key of ['s', 'd', 'i', '1', '2', '3']) await page.keyboard.press(key);
  expect(await saved(page)).toEqual(before);
  await nav(page, 'Tree').click();
  await expect(page.locator('.tree-canvas')).toBeVisible();
  await expect(page.locator('.tree-tooltip, .attr-picker, .node-planned-add, .node-planned-remove')).toHaveCount(0);
  expect(await saved(page)).toEqual(before);
});

async function pauseAfterRealEvaluation(page: Page, planner: Planner) {
  // Suspend only the scheduler's inter-batch yield. No scoring/response replacement,
  // candidate truncation, time-based sleeps or synthetic progress callbacks.
  await page.clock.install({ time: new Date('2026-01-01T00:00:00Z') });
  await page.clock.pauseAt(new Date('2026-01-01T00:00:01Z'));
  await run(page, planner).click({ force: true });
  const progress = page.locator('.opt-progress progress');
  await expect.poll(async () => {
    await page.clock.runFor(1);
    if (!await progress.count()) return 0;
    return Number(await progress.getAttribute('value'));
  }, { timeout: 90_000, intervals: [10] }).toBeGreaterThan(0);
  expect(Number(await progress.getAttribute('value')), 'Pause an evaluated, incomplete real-WASM search')
    .toBeLessThan(Number(await progress.getAttribute('max')));
  await expect(results(page, planner)).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Cancel', exact: true })).toBeVisible();
}

for (const planner of ['route', 'joint'] as const) {
  for (const action of ['cancel', 'leave', 'hidden goal'] as const) {
    test(`${planner} ${action} rejects late real-backend continuation and requires explicit restart`, async ({ page }) => {
      await syntheticWitch(page);
      await configure(page);
      await budget(page).fill('8');
      await addPlacement(page);
      const before = await saved(page);
      const beforeLife = await life(page).innerText();
      await pauseAfterRealEvaluation(page, planner);
      if (action === 'cancel') {
        await page.getByRole('button', { name: 'Cancel', exact: true }).click({ force: true });
      } else {
        await nav(page, action === 'leave' ? 'Calcs' : 'Upgrades').click({ force: true });
        await expect(page.locator('.tree-canvas, .tree-planner-run, .opt-progress')).toHaveCount(0);
        if (action === 'hidden goal') {
          await page.getByRole('button', { name: 'Max total DPS', exact: true }).click({ force: true });
          await page.getByRole('checkbox', { name: 'Prioritize capped elemental resistances', exact: true }).check({ force: true });
        }
      }
      // Drain the continuation following actual WASM candidate work after cancellation.
      await page.clock.runFor(2000);
      if (action !== 'cancel') await nav(page, 'Tree').click({ force: true });
      await noReceipts(page);
      await page.clock.runFor(2000);
      await noReceipts(page);
      await expect(budget(page)).toHaveValue('8');
      await expect(joint(page).locator('.jewel-plan-candidates > li')).toHaveCount(1);
      expect(await saved(page)).toEqual(before);
      await expect(life(page)).toHaveText(beforeLife);
      if (action === 'hidden goal') {
        await expect(page.getByRole('button', { name: 'Objective', exact: true })).toContainText('Max total DPS');
        await expect(page.getByRole('checkbox', { name: 'Prioritize capped elemental resistances', exact: true })).toBeChecked();
        // The fresh run below intentionally uses a newly chosen shared objective.
        await select(page, 'Objective', 'Max Life');
        await page.getByRole('checkbox', { name: 'Prioritize capped elemental resistances', exact: true }).uncheck();
      }
      await page.clock.resume();
      await budget(page).fill('3');
      await complete(page, planner);
      expect(await saved(page)).toEqual(before);
    });
  }
}

for (const planner of ['route', 'joint'] as const) {
  test(`${planner} retired generation cannot clear an overlapping new search`, async ({ page }) => {
    await syntheticWitch(page);
    await configure(page);
    await addPlacement(page);
    const before = await saved(page);
    // Hold only the existing inter-batch yield after real WASM work. Both
    // generations keep the real request, scoring and progress publication.
    await page.evaluate(() => {
      const state = window as Window & { passiveHold: boolean; passiveReleases: (() => void)[] };
      const schedule = window.setTimeout.bind(window);
      state.passiveHold = true;
      state.passiveReleases = [];
      window.setTimeout = (callback, delay, ...args) => {
        const progress = document.querySelector<HTMLProgressElement>('.opt-progress progress');
        if (state.passiveHold && delay === 0 && typeof callback === 'function'
          && progress && progress.value > 0 && progress.value < progress.max) {
          state.passiveHold = false;
          state.passiveReleases.push(() => callback(...args));
          return schedule(() => {}, 0);
        }
        return schedule(callback, delay, ...args);
      };
    });
    await run(page, planner).click();
    await expect.poll(() => page.evaluate(() =>
      (window as Window & { passiveReleases: (() => void)[] }).passiveReleases.length)).toBe(1);
    await nav(page, 'Calcs').click();
    await nav(page, 'Tree').click();
    await noReceipts(page);
    await page.evaluate(() => { (window as Window & { passiveHold: boolean }).passiveHold = true; });
    await run(page, planner).click();
    await expect.poll(() => page.evaluate(() =>
      (window as Window & { passiveReleases: (() => void)[] }).passiveReleases.length)).toBe(2);
    const progress = page.locator('.opt-progress progress');
    const pendingValue = await progress.getAttribute('value');
    await page.evaluate(async () => {
      (window as Window & { passiveReleases: (() => void)[] }).passiveReleases[0]();
      await new Promise<void>(resolve => window.setTimeout(resolve, 0));
    });
    await expect(progress).toHaveAttribute('value', pendingValue!);
    await expect(page.getByRole('button', { name: 'Cancel', exact: true })).toBeVisible();
    await expect(results(page, planner)).toHaveCount(0);
    await page.evaluate(() => {
      (window as Window & { passiveReleases: (() => void)[] }).passiveReleases[1]();
    });
    await expect(results(page, planner)).toBeVisible({ timeout: 90_000 });
    await expect(page.locator('.opt-progress')).toHaveCount(0);
    expect(await saved(page)).toEqual(before);
  });
}

test('historical tree replacement discards current-tree receipts and exposes no interactive planner', async ({ page }) => {
  await syntheticWitch(page);
  await configure(page);
  await addPlacement(page);
  await complete(page, 'joint');
  await complete(page, 'route');
  await nav(page, 'Build').click();
  // Synthetic PoB document only; decoding/calculation/version rejection remain real.
  const historical = deflateSync('<PathOfBuilding2><Build level="80" className="Witch"/><Tree activeSpec="1"><Spec nodes="770" treeVersion="0_1"/></Tree><Skills/><Items/></PathOfBuilding2>').toString('base64url');
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(historical);
  await page.locator('.import-submit').click();
  await expect.poll(async () => (await saved(page)).treeVersion).toBe('0_1');
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await nav(page, 'Tree').click();
  await expect(page.getByRole('main').getByText('Interactive tree editing and route planning are unavailable for historical trees.', { exact: false })).toBeVisible();
  await expect(page.locator('.tree-canvas, .tree-planner, .jewel-plan-results')).toHaveCount(0);
  const before = await saved(page);
  await awayHistorical(page);
  expect(await saved(page)).toEqual(before);
});

async function awayHistorical(page: Page) {
  await nav(page, 'Calcs').click();
  await nav(page, 'Tree').click();
  await expect(page.locator('.tree-canvas, .tree-planner')).toHaveCount(0);
}
