import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import type { PassiveNode } from '../src/api/types';
import { allocationPaths, passivePlanningContext } from '../src/lib/passivePlanner';
import { busy } from './shell';
import { choose } from './treePage';

const version = readFileSync(new URL('../../data/CURRENT', import.meta.url), 'utf8').trim();
const nodes = JSON.parse(readFileSync(new URL(`../../data/${version}/base/passive_tree.json`, import.meta.url), 'utf8')) as PassiveNode[];
const context = passivePlanningContext(nodes, [], 'Witch');
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
  await page.locator('.node-power > summary').click();
}

const saved = (page: Page) => page.evaluate(() => localStorage.getItem('pobr-v2-build-state'));

test('report filters, scores full paths, sorts and focuses without allocating; stat changes invalidate scores', async ({page}) => {
  await ready(page);
  const before = await saved(page);
  const report = page.locator('.node-power');
  await report.getByRole('searchbox', {name:'Filter nodes'}).fill(String(target.skill));
  const row = report.locator(`tr[data-node-id="${target.skill}"]`);
  await expect(row).toHaveAttribute('data-status','pending');
  await report.getByRole('button',{name:'Evaluate next 128 nodes',exact:true}).click();
  await expect(row).toHaveAttribute('data-status','ready',{timeout:60_000});
  await expect(row.locator('td').nth(2)).toHaveText(String(route.allocate.length));
  await expect(row.locator('td').nth(1)).not.toHaveText('—');
  await report.getByRole('button',{name:'Points',exact:true}).click();
  await expect(report.getByRole('columnheader',{name:'Points',exact:true})).toHaveAttribute('aria-sort','ascending');
  await row.getByRole('button', {name:target.name!,exact:true}).click();
  await expect(page.locator('.tree-node-details')).toHaveAttribute('data-skill-id',String(target.skill));
  const view = (await page.locator('.tree-canvas svg').getAttribute('viewBox'))!.split(' ').map(Number);
  expect(target.x!).toBeGreaterThanOrEqual(view[0]); expect(target.x!).toBeLessThanOrEqual(view[0]+view[2]);
  expect(target.y!).toBeGreaterThanOrEqual(view[1]); expect(target.y!).toBeLessThanOrEqual(view[1]+view[3]);
  expect(await saved(page)).toBe(before);
  await choose(page, report.getByRole('combobox',{name:'Report stat',exact:true}), 'Mana');
  await expect(row).toHaveAttribute('data-status','pending');
  await expect(row.locator('td').nth(1)).toHaveText('—');
  await report.getByRole('button',{name:'Evaluate next 128 nodes',exact:true}).click();
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
  await expect(report.locator('tbody tr')).toHaveCount(0);
  await report.getByRole('checkbox',{name:'Include small nodes'}).check();
  await expect(report.locator('tbody tr').first()).toBeVisible();
  await report.getByRole('searchbox',{name:'Filter nodes'}).fill('');
  await report.getByRole('button',{name:'Evaluate next 128 nodes',exact:true}).click();
  await report.getByRole('button',{name:'Cancel report',exact:true}).click();
  await expect(report.getByRole('status')).toHaveText('Report cancelled; completed rows retained.');
  await expect(report.getByRole('button',{name:'Cancel report',exact:true})).toHaveCount(0);
  await choose(page, report.getByRole('combobox',{name:'Report stat',exact:true}), 'Energy Shield');
  await expect(report.locator('tr[data-status=ready]')).toHaveCount(0);
  await report.locator('summary').first().click();
  await report.locator('summary').first().click();
  await expect(report.locator('tr[data-status=ready]')).toHaveCount(0);
  expect(await saved(page)).toBe(before);
});
