import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import type { PassiveNode } from '../src/api/types';
import { allocationPaths, passivePlanningContext } from '../src/lib/passivePlanner';
import { busy } from './shell';
import { choose } from './treePage';

const version = readFileSync(new URL('../../data/CURRENT', import.meta.url), 'utf8').trim();
const nodes = JSON.parse(readFileSync(new URL(`../../data/${version}/base/passive_tree.json`, import.meta.url), 'utf8')) as PassiveNode[];
const jewels = JSON.parse(readFileSync(new URL(`../../data/${version}/overlay/passive_jewels.json`, import.meta.url), 'utf8')) as { class_starts: Record<string, string> };
// Same resolution as the engine's tree_effects.class_starts: the lowest node skill with that id.
const classStarts = Object.fromEntries(Object.entries(jewels.class_starts).map(([name, id]) =>
  [name, Math.min(...nodes.filter(node => node.id === id).map(node => node.skill))]));
const treeEffects = { class_starts: classStarts, allocation_grants: [], nodes: {}, conquered: [], unresolved: [], rings: [], warnings: [] };
const context = passivePlanningContext(nodes, [], 'Witch', [], [], undefined, treeEffects);
const route = allocationPaths(context, context.allocated, 5).find(p => p.target > 10000 && context.byId.get(p.target)?.kind === 'notable')!;
const target = context.byId.get(route.target)!;

test.use({ viewport: { width: 1440, height: 1000 } });
async function ready(page: Page) {
  await page.addInitScript(() => {
    localStorage.setItem('pobr-v2-build-state', JSON.stringify({ version:1, notes:'Power test', state:{
      pobCode:null, character:{level:80,class_name:'Witch',ascendancy_name:''}, allocatedNodes:[], attributeChoices:{},
      socketGroups:[{enabled:true,gems:[{skill_id:'FireballPlayer',level:16,quality:0}]}], items:[],flasks:[],limbs:[],jewels:[], annotations:{},params:{config_inputs:{}}
    }}));
  });
  await page.goto('/tree');
  await expect(page.locator('.tree-canvas svg')).toBeVisible({timeout:90_000});
  await expect(busy(page)).toHaveCount(0);
  await expect(page.locator('.node-power')).toHaveAttribute('open', '');
  await expect(page.locator('.node-power-results')).toHaveCount(0);
}

const saved = (page: Page) => page.evaluate(() => localStorage.getItem('pobr-v2-build-state'));

test('report filters, scores full paths, sorts and focuses without allocating; stat changes invalidate scores', async ({page}) => {
  await ready(page);
  const before = await saved(page);
  const report = page.locator('.node-power');
  await report.getByRole('searchbox', {name:'Filter nodes'}).fill(String(target.skill));
  const row = report.locator(`[data-node-id="${target.skill}"]`);
  await expect(report).toContainText('Run an evaluation to see candidate rankings');
  await report.getByRole('button',{name:'Evaluate all candidate nodes',exact:true}).click();
  await expect(row).toHaveAttribute('data-status','ready',{timeout:60_000});
  await expect(row.locator('.node-power-route > summary')).toContainText(`Path cost: ${route.allocate.length}`);
  await expect(row.locator('.node-power-value')).not.toContainText('—');
  await report.getByRole('button',{name:'Points',exact:true}).click();
  await expect(report.getByRole('button', {name: /Points.*Ascending/})).toHaveAttribute('aria-pressed', 'true');
  await row.locator('.node-power-route > summary').click();
  await expect(row.getByText(/gain for single node/)).toBeVisible();
  await row.getByRole('button', {name:target.name!,exact:true}).click();
  await expect(page.locator('.tree-node-details')).toHaveAttribute('data-skill-id',String(target.skill));
  const view = (await page.locator('.tree-canvas svg').getAttribute('viewBox'))!.split(' ').map(Number);
  expect(target.x!).toBeGreaterThanOrEqual(view[0]); expect(target.x!).toBeLessThanOrEqual(view[0]+view[2]);
  expect(target.y!).toBeGreaterThanOrEqual(view[1]); expect(target.y!).toBeLessThanOrEqual(view[1]+view[3]);
  expect(await saved(page)).toBe(before);
  await choose(page, report.getByRole('combobox',{name:'Report stat',exact:true}), 'Mana');
  await expect(report).toContainText('Run an evaluation to see candidate rankings');
  await expect(row).toHaveCount(0);
  await report.getByRole('button',{name:'Evaluate all candidate nodes',exact:true}).click();
  await expect(row).toHaveAttribute('data-status','ready');
  await report.screenshot({path:'test-results/node-power-report.png'});
  await page.screenshot({path:'test-results/node-power-tree.png',fullPage:true});
  expect(await saved(page)).toBe(before);
});

test('small-node toggle and cancellation preserve the build and never restore stale scores', async ({page}) => {
  await ready(page);
  const before = await saved(page);
  const report = page.locator('.node-power');
  await report.getByRole('searchbox',{name:'Filter nodes'}).fill(String(nodes.find(n => n.name === 'Attribute' && n.skill >= 10000)!.skill));
  await expect(report.locator('[data-node-id]')).toHaveCount(0);
  await report.getByRole('checkbox',{name:'Include small nodes'}).check();
  await expect(report).toContainText('Run an evaluation to see candidate rankings');
  await expect(report.getByRole('button', { name: 'Evaluate all candidate nodes', exact: true })).toBeEnabled();
  await report.getByRole('searchbox',{name:'Filter nodes'}).fill('');
  await report.getByRole('button',{name:'Evaluate all candidate nodes',exact:true}).click();
  await report.getByRole('button',{name:'Cancel report',exact:true}).click();
  await expect(report.getByRole('status')).toHaveText('Report canceled; rankings are provisional. Completed rows retained.');
  await expect(report.getByRole('button',{name:'Cancel report',exact:true})).toHaveCount(0);
  await choose(page, report.getByRole('combobox',{name:'Report stat',exact:true}), 'Energy Shield');
  await expect(report.locator('[data-status=ready]')).toHaveCount(0);
  await report.locator('summary').first().click();
  await report.locator('summary').first().click();
  await expect(report.locator('[data-status=ready]')).toHaveCount(0);
  expect(await saved(page)).toBe(before);
});

for (const width of [390, 1440, 2560]) {
  test(`report expansion keeps sidebar width and page bounds at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 1000 });
    await ready(page);
    const report = page.locator('.node-power');
    const sidebar = page.locator('.tree-side');
    const expandedWidth = (await sidebar.boundingBox())!.width;
    await report.locator(':scope > summary').click();
    expect((await sidebar.boundingBox())!.width).toBeCloseTo(expandedWidth, 0);
    await report.locator(':scope > summary').click();
    expect((await sidebar.boundingBox())!.width).toBeCloseTo(expandedWidth, 0);
    await report.getByRole('searchbox', { name: 'Filter nodes' }).fill(String(target.skill));
    await report.getByRole('button', { name: 'Evaluate all candidate nodes', exact: true }).click();
    await expect(report.locator(`[data-node-id="${target.skill}"]`)).toHaveAttribute('data-status', 'ready', { timeout: 60_000 });
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    const heading = report.locator('.node-power-heading').first();
    const name = (await heading.getByRole('button').boundingBox())!;
    const gain = (await heading.locator('strong').boundingBox())!;
    expect(name.x + name.width).toBeLessThanOrEqual(gain.x);
    expect(Math.abs(name.y - gain.y)).toBeLessThan(8);
    expect(await report.evaluate(element => element.scrollWidth <= element.clientWidth + 1)).toBe(true);
    expect((await sidebar.boundingBox())!.width).toBeCloseTo(expandedWidth, 0);
  });
}
