import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import type { PassiveNode, PassiveTreeMeta } from '../src/api/types';
import { defaultStarterSession } from '../src/lib/buildWorkspace';
import { allocationRoutes, buildPassiveGraph } from '../src/lib/passiveGraph';
import { hasPosition, isAttributeNode, treeExtent } from '../src/lib/treeGeometry';
import { busy, pageLink } from './shell';
import { choose } from './treePage';

const version = readFileSync(new URL('../../data/CURRENT', import.meta.url), 'utf8').trim();
const nodes: PassiveNode[] = JSON.parse(readFileSync(new URL(`../../data/${version}/base/passive_tree.json`, import.meta.url), 'utf8'));
const metadata: PassiveTreeMeta = JSON.parse(readFileSync(new URL(`../../data/${version}/base/passive_tree_meta.json`, import.meta.url), 'utf8'));
const characterClass = metadata.classes.find(entry => (entry.ascendancies?.length ?? 0) >= 2)!;
const [firstAscendancy, secondAscendancy] = characterClass.ascendancies!;
const svg = (page: Page) => page.locator('.tree-canvas svg');
const ascendancyPicker = (page: Page) => page.locator('.tree-asc-picker').nth(1).getByRole('combobox');
const reset = (page: Page) => page.getByRole('button', { name: 'Reset view', exact: true });
const focusAscendancy = (page: Page) => page.getByRole('button', { name: 'Focus ascendancy', exact: true });

function overview(ascendancy = '') {
  const { x: left, y: top, w: width, h: height } = treeExtent(nodes.filter(node => !node.ascendancy_id || node.ascendancy_id === ascendancy))!;
  return `${left} ${top} ${width} ${height}`;
}

function routeToSocket() {
  const { class_starts: classStarts } = JSON.parse(readFileSync(new URL(`../../data/${version}/overlay/passive_jewels.json`, import.meta.url), 'utf8')) as { class_starts: Record<string, string> };
  const startIds = new Set(Object.values(classStarts));
  const root = Math.min(...nodes.filter(node => node.id === classStarts[characterClass.name.toLowerCase()]).map(node => node.skill));
  const graph = buildPassiveGraph(nodes.filter(node => !node.ascendancy_id && node.kind !== 'mastery' && !node.unlock_constraint
    && (!startIds.has(node.id) || node.skill === root)));
  const routes = allocationRoutes(graph, new Set(), root);
  return nodes.filter(hasPosition).filter(node => node.kind === 'jewel_socket' && routes.has(node.skill))
    .map(node => ({ node, route: routes.get(node.skill)! }))
    .sort((left, right) => left.route.length - right.route.length || left.node.skill - right.node.skill)[0];
}

async function expectAscendancyView(page: Page, ascendancy: string) {
  const expected = treeExtent(nodes.filter(node => node.ascendancy_id === ascendancy), 700)!;
  await expect.poll(async () => (await svg(page).getAttribute('viewBox'))?.split(' ').map(Number))
    .toEqual([expected.x, expected.y, expected.w, expected.h].map(value => expect.closeTo(value, 6)));
}

async function seed(page: Page, ascendancy = firstAscendancy.name, allocatedNodes: number[] = []) {
  const saved = defaultStarterSession(characterClass.name);
  saved.state.character.level = 90;
  saved.state.character.ascendancy_name = ascendancy;
  saved.state.allocatedNodes = allocatedNodes;
  saved.state.attributeChoices = Object.fromEntries(nodes.filter(node => isAttributeNode(node) && allocatedNodes.includes(node.skill))
    .map(node => [String(node.skill), 'str' as const]));
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.addInitScript(saved => {
    if (!localStorage.getItem('pobr-v2-build-state')) localStorage.setItem('pobr-v2-build-state', JSON.stringify(saved));
  }, saved);
}

async function ready(page: Page) {
  await expect(svg(page)).toBeVisible({ timeout: 90_000 });
  await expect(ascendancyPicker(page)).toBeEnabled();
  await expect(busy(page)).toHaveCount(0);
}

async function deferTreeLoad(page: Page) {
  await page.addInitScript(() => {
    const probe = { pending: 0, release: () => {} };
    Object.assign(window, { treeLoadProbe: probe });
    const NativeWorker = window.Worker;
    window.Worker = class extends NativeWorker {
      constructor(url: string | URL, options?: WorkerOptions) {
        super(url, options);
        const treeCalls = new Set<number>();
        const post = this.postMessage;
        this.postMessage = (message: { id: number; method: string }, ...rest: unknown[]) => {
          if (message.method === 'loadPassiveTree') treeCalls.add(message.id);
          return Reflect.apply(post, this, [message, ...rest]);
        };
        this.addEventListener('message', event => {
          if (!treeCalls.delete(event.data.id)) return;
          event.stopImmediatePropagation();
          probe.pending++;
          const previous = probe.release;
          probe.release = () => {
            previous();
            this.dispatchEvent(new MessageEvent('message', { data: event.data }));
          };
        });
      }
    };
  });
}

test.use({ viewport: { width: 1440, height: 1000 } });

test('a loaded tree opens at the overview and explicit focus, reset and locating still navigate', async ({ page }) => {
  const { node: socket, route } = routeToSocket();
  await seed(page, firstAscendancy.name, route);
  await page.goto('/tree');
  await ready(page);
  await expect(svg(page)).toHaveAttribute('viewBox', overview(firstAscendancy.id));
  await pageLink(page, 'Items').click();
  await expect(svg(page)).toHaveCount(0);
  await pageLink(page, 'Tree').click();
  await expect(svg(page)).toHaveAttribute('viewBox', overview(firstAscendancy.id));

  await focusAscendancy(page).click();
  await expectAscendancyView(page, firstAscendancy.id);
  await reset(page).click();
  await expect(svg(page)).toHaveAttribute('viewBox', overview(firstAscendancy.id));

  await page.getByRole('button', { name: 'Plan passive upgrades', exact: true }).click();
  const planner = page.locator('.jewel-passive-planner');
  await planner.getByRole('combobox', { name: 'Candidate socket', exact: true }).click();
  await page.locator(`[role="option"][data-value="${socket.skill}"]`).click();
  await planner.getByRole('button', { name: 'Locate on passive tree', exact: true }).click();
  await expect(page.locator('.tree-node-details')).toHaveAttribute('data-skill-id', String(socket.skill));
  const located = (await svg(page).getAttribute('viewBox'))!.split(' ').map(Number);
  expect(located[0] + located[2] / 2).toBeCloseTo(socket.x, 6);
  expect(located[1] + located[3] / 2).toBeCloseTo(socket.y, 6);
  await reset(page).click();
  await expect(svg(page)).toHaveAttribute('viewBox', overview(firstAscendancy.id));
  await pageLink(page, 'Items').click();
  await pageLink(page, 'Tree').click();
  await expect(svg(page)).toHaveAttribute('viewBox', overview(firstAscendancy.id));
  await expect(page.locator('.tree-node-details')).toHaveCount(0);
});

test('a restored ascendancy stays in the overview when passive nodes arrive asynchronously', async ({ page }) => {
  await seed(page);
  await deferTreeLoad(page);
  await page.goto('/tree');
  await expect.poll(() => page.evaluate(() => (window as unknown as { treeLoadProbe: { pending: number } }).treeLoadProbe.pending),
    { timeout: 90_000 }).toBeGreaterThan(0);
  await expect(page.getByText('Loading tree…', { exact: true })).toBeVisible();
  await expect(svg(page)).toHaveCount(0);
  await page.evaluate(() => (window as unknown as { treeLoadProbe: { release: () => void } }).treeLoadProbe.release());
  await ready(page);
  await expect(svg(page)).toHaveAttribute('viewBox', overview(firstAscendancy.id));
  await expect(focusAscendancy(page)).toBeVisible();
});

test('explicit ascendancy selections focus once across canvas remounts and clearing restores the main tree', async ({ page }) => {
  await seed(page, '');
  await page.goto('/tree');
  await ready(page);
  await expect(svg(page)).toHaveAttribute('viewBox', overview());
  await expect(focusAscendancy(page)).toHaveCount(0);

  for (const selected of [firstAscendancy, secondAscendancy]) {
    await choose(page, ascendancyPicker(page), selected.name);
    await ready(page);
    await expect(ascendancyPicker(page)).toHaveText(selected.name);
    await expectAscendancyView(page, selected.id);
    await reset(page).click();
    await expect(svg(page)).toHaveAttribute('viewBox', overview(selected.id));
    await pageLink(page, 'Items').click();
    await pageLink(page, 'Tree').click();
    await expect(svg(page)).toHaveAttribute('viewBox', overview(selected.id));
  }

  await choose(page, ascendancyPicker(page), '(none)');
  await ready(page);
  await expect(svg(page)).toHaveAttribute('viewBox', overview());
  await expect(focusAscendancy(page)).toHaveCount(0);
});
