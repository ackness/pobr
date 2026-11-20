import { readFileSync } from 'node:fs';
import { t } from '../src/lib/i18n';
import { expect, test, type Page } from '@playwright/test';
import { openItemsAfterImport } from './shell';
import { importBuild } from './skillsAndCalcs';
import { headlineDpsValue, goTo, openOptimizerSettings, settledNumber, settledText } from './upgradesPage';
import { SMOKE } from './tags';

async function caster(page: Page) {
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: 85, class_name: 'Sorceress' },
    equipments: [{ inventoryId: 'Ring', x: 0, baseType: 'Sapphire Ring', name: 'Current Ring', frameType: 2,
      explicitMods: ['40% increased Fire Damage', '+80 to maximum Life'] }],
    talent_tree: { hashes: [54447, 4739, 22419, 18407], quest_stats: [] }, jewel_data: '[]',
    skills: [{ baseType: 'Fireball', support: false, properties: [{ type: 5, values: [['16', 0]] }] }],
  } }));
  await page.goto('/build');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticGuidanceFixture');
  await page.locator('.import-submit').click();
  await openItemsAfterImport(page);
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 30_000 });
  await page.getByRole('combobox', { name: t('en-US', 'sidebar.mainSkill'), exact: true }).click();
  await page.getByRole('option', { name: /Fireball.*Group 1/ }).click();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
}
const dps = headlineDpsValue;

test('current market Sum is a reference; complete replacement detects a downgrade before applying', SMOKE, async ({ page }) => {
  await caster(page);
  const before = await settledText(dps(page));
  await page.setViewportSize({ width: 390, height: 844 });
  await goTo(page, 'Upgrades');
  const pasteAction = page.getByRole('button', { name: 'Paste item to compare', exact: true });
  const slotAction = page.getByRole('button', { name: 'Find upgrades by slot', exact: true });
  await expect(pasteAction).toBeInViewport();
  await expect(slotAction).toBeInViewport();
  await pasteAction.click();
  const pasted = page.getByRole('textbox', { name: 'Complete item text' });
  await expect(pasted).toBeFocused();
  await expect(pasted).toBeInViewport();
  await slotAction.click();
  await expect(page.locator('.trade-workspace')).toBeFocused();
  await page.locator('.trade-position').filter({ hasText: /^Ring 1/ }).click();
  await page.getByRole('button', { name: 'Calculate modifier scores', exact: true }).click();
  const reference = page.locator('.upgrade-score-reference');
  await expect(reference).toBeVisible();
  await expect(reference).toContainText('Current item’s market weight total');
  await expect(page.locator('.trade-score-table')).toContainText('Relative filter weight');
  const sum = Number((await reference.locator('div > strong').first().innerText()).replaceAll(',', ''));
  const href = await page.locator('.trade-market-link').first().getAttribute('href');
  const query = JSON.parse(new URL(href!).searchParams.get('q')!);
  expect(sum).toBeGreaterThan(0);
  expect(Math.round(query.query.stats[0].value.min)).toBe(sum);
  await page.getByRole('button', { name: 'Compare a complete item', exact: true }).click();
  await expect(pasted).toBeFocused();
  await pasted.fill('Rarity: NORMAL\nSapphire Ring');
  await page.getByRole('button', { name: 'Calculate replacement' }).click();
  await expect(page.locator('.replacement-verdict')).toContainText('Whole-item comparison · Ring 1');
  await expect(page.locator('.upgrade-replacement-result')).toContainText('Combined DPS decreases after this replacement');
  await expect(reference).toHaveCount(1);
  await expect(dps(page)).toHaveText(before);
  const preview = await page.locator('.swap-gain-card [data-stat="CombinedDPS"] .swap-after').innerText();
  await page.getByRole('button', { name: 'Apply to build · Ring 1', exact: true }).click();
  await expect(dps(page)).not.toHaveText(before);
  await expect.poll(async () => Number((await dps(page).innerText()).replaceAll(',', ''))).toBeCloseTo(Number(preview.replaceAll(',', '')), 0);
  await expect(reference).toHaveCount(0);
  await page.getByRole('textbox', { name: 'Complete item text' }).fill('Rarity: NORMAL\nTwin Bow');
  await page.getByRole('button', { name: 'Calculate replacement' }).click();
  await expect(page.getByRole('alert')).toContainText('No compatible equipped position');
  await expect(page.getByRole('button', { name: 'Open Items to equip an empty slot' })).toBeVisible();
  await expect(page.getByRole('button', { name: /Apply to build/ })).toHaveCount(0);
});

test('shared goals link automatic supports and connected passive plans with real apply', async ({ page }) => {
  await caster(page);
  await goTo(page, 'Upgrades');
  await page.getByRole('checkbox', { name: 'Prioritize capped elemental resistances' }).check();
  await page.locator('.upgrade-paths > button').filter({ hasText: 'opens Skills' }).click();
  await expect(page.locator('.gem-optimizer-body')).toBeVisible();
  await openOptimizerSettings(page);
  await expect(page.getByRole('checkbox', { name: 'Prioritize capped elemental resistances' })).toBeChecked();
  await expect.poll(async () => Number(await page.locator('.support-pool-summary strong').innerText())).toBeGreaterThan(20);
  await page.getByRole('button', { name: 'Calculate support combinations', exact: true }).click();
  const support = page.locator('.support-plan').first();
  await expect(support).toBeVisible({ timeout: 30_000 });
  await expect(support.locator('.support-plan-metrics')).toContainText('Effective HP');
  const before = await settledText(dps(page));
  await support.getByRole('button', { name: 'Apply supports', exact: true }).click();
  await expect(dps(page)).not.toHaveText(before);
  await goTo(page, 'Upgrades');
  await page.locator('.trade-position').filter({ hasText: 'Skill and support gems' }).click();
  await page.getByRole('button', { name: 'Calculate modifier scores', exact: true }).click();
  await expect(page.locator('.trade-gems')).toBeVisible();
  for (const card of await page.locator('.trade-gem-card').filter({ hasText: 'Skill adjustment' }).all()) {
    await expect(card.getByRole('link')).toHaveCount(0);
  }
  await page.locator('.upgrade-paths > button').filter({ hasText: 'opens Passive tree' }).click();
  await expect(page.locator('.tree-planner-body')).toBeVisible();
  await expect(page.getByRole('checkbox', { name: 'Prioritize capped elemental resistances' })).toBeChecked();
  await page.getByRole('button', { name: 'Find upgrade routes', exact: true }).click();
  const route = page.locator('.tree-planner-results > li').first();
  await expect(route).toBeVisible({ timeout: 30_000 });
  await route.getByRole('button', { name: 'Show on tree' }).click();
  await expect(page.locator('.node-planned-add').first()).toBeVisible();
  const treeBefore = await settledText(dps(page));
  await route.getByRole('button', { name: 'Apply plan' }).click();
  await expect(page.locator('.tree-planner-results > li')).toHaveCount(0);
  await expect(dps(page)).not.toHaveText(treeBefore);
  await expect(page.locator('.shell-alert')).toHaveCount(0);
});

test('support exclusions persist, invalidate old plans and constrain real-WASM recommendations', async ({ page }, testInfo) => {
  const openOptimizer = async () => {
    await goTo(page, 'Skills');
    await expect(page.locator('.skills-toolbar input')).toBeEnabled();
    const group = page.locator('.skill-group-title').first();
    if (await group.getAttribute('aria-expanded') === 'false') await group.click();
    const toggle = page.locator('.gem-optimizer-toggle');
    if (await toggle.getAttribute('aria-expanded') === 'false') await toggle.click();
    await openOptimizerSettings(page);
  };
  await caster(page);
  const baseline = await settledText(dps(page));
  await goTo(page, 'Upgrades');
  await page.locator('.upgrade-paths > button').filter({ hasText: 'opens Skills' }).click();
  await openOptimizerSettings(page);
  const run = page.getByRole('button', { name: 'Calculate support combinations', exact: true });
  const count = page.locator('.support-pool-summary strong');
  await expect.poll(async () => Number(await count.innerText())).toBeGreaterThan(20);
  const candidates = Number(await count.innerText());
  await run.click();
  const plan = page.locator('.support-plan').first();
  await expect(plan).toBeVisible({ timeout: 30_000 });
  const details = plan.locator('.support-plan-details');
  const gem = details.locator('.support-plan-gems > .opt-chip').first();
  await expect(gem).toBeHidden();
  await expect(plan.locator('.support-plan-edits')).toBeVisible();
  const summary = details.locator('summary').first();
  await summary.focus();
  await summary.press('Enter');
  await expect(gem).toBeVisible();
  const id = (await gem.getAttribute('data-skill-id'))!;
  const name = (await gem.getByRole('button').getAttribute('aria-label'))!.replace(/^Exclude /, '');
  await gem.getByRole('button').click();
  await expect(page.locator('.support-results')).toHaveCount(0);
  await expect(count).toHaveText(String(candidates - 1));
  await expect(page.locator('.support-excluded')).toContainText(name);
  await expect(dps(page)).toHaveText(baseline);
  await page.locator('.support-pool-details > summary').click();
  const search = page.getByRole('searchbox', { name: 'Search supports' });
  await search.fill(name);
  const checkbox = page.locator(`.support-candidate[data-skill-id="${id}"]`).getByRole('checkbox');
  await expect(checkbox).not.toBeChecked();
  await run.click();
  await expect(page.locator('.support-plan').first()).toBeVisible({ timeout: 30_000 });
  await expect(page.locator(`.support-plan-gems > [data-skill-id="${id}"]`)).toHaveCount(0);
  await search.fill('no such support name');
  await expect(page.locator('.support-candidate')).toHaveCount(0);
  await expect(page.locator('.support-plan').first()).toBeVisible(); // Searching does not alter the selected pool.
  await expect(dps(page)).toHaveText(baseline);
  for (const width of [320, 390, 768, 1440, 2560]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.locator('main').evaluate(element => element.scrollWidth - element.clientWidth);
    expect(overflow, `Support exclusions and results must fit ${width}px`).toBeLessThanOrEqual(1);
    if ([390, 1440, 2560].includes(width)) {
      await plan.scrollIntoViewIfNeeded();
      await page.screenshot({ path: testInfo.outputPath(`support-recommendations-${width}.png`) });
    }
  }

  await goTo(page, 'Items');
  await openOptimizer();
  await expect(page.locator('.support-excluded')).toContainText(name);
  await expect(count).toHaveText(String(candidates - 1));
  await page.reload();
  await openOptimizer();
  await expect(page.locator('.support-excluded')).toContainText(name);
  await expect(count).toHaveText(String(candidates - 1));
  await page.getByRole('button', { name: `Restore ${name}`, exact: true }).click();
  await expect(page.locator('.support-excluded')).toHaveCount(0);
  await expect(count).toHaveText(String(candidates));
  await expect(dps(page)).toHaveText(baseline);

  await page.locator('.support-pool-details > summary').click();
  const ids = await page.locator('.support-candidate').evaluateAll(elements => elements.map(element => element.getAttribute('data-skill-id')!));
  await page.evaluate(ids => {
    const key = Object.keys(localStorage).find(key => key.startsWith('pobr-v2-support-exclusions:'))!;
    localStorage.setItem(key, JSON.stringify(ids));
  }, ids);
  await page.reload();
  await openOptimizer();
  await expect(count).toHaveText('0');
  await expect(run).toBeDisabled();
  await expect(page.locator('.gem-optimizer-body')).toContainText('All eligible supports are excluded');
  await page.getByRole('button', { name: 'Restore all excluded supports' }).click();
  await expect(count).toHaveText(String(candidates));
  await expect(run).toBeEnabled();
});

test('tree panning keeps the viewport fixed, reveals edge nodes and commits without a jump', async ({ page }) => {
  await caster(page);
  await page.setViewportSize({ width: 1440, height: 1000 });
  await goTo(page, 'Tree');
  const svg = page.locator('.tree-canvas svg');
  await expect(svg).toBeVisible();
  await svg.scrollIntoViewIfNeeded();
  const box = (await svg.boundingBox())!;
  const start = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  await page.mouse.move(start.x, start.y);
  for (let i = 0; i < 7; i++) {
    await page.mouse.wheel(0, -100);
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  }
  const initial = await svg.getAttribute('viewBox');
  const edgeId = await svg.evaluate(element => {
    const rect = element.getBoundingClientRect();
    return [...element.querySelectorAll('circle[data-skill-id]')].find(node => {
      const b = node.getBoundingClientRect();
      return b.x < rect.x - 10 && b.x > rect.x - 100 && b.y > rect.y + 100 && b.bottom < rect.bottom - 120;
    })?.getAttribute('data-skill-id');
  });
  expect(edgeId, 'Zoomed tree should have nodes just beyond the left edge').toBeTruthy();
  const node = page.locator(`circle[data-skill-id="${edgeId}"]`);
  await page.mouse.down();
  await page.mouse.move(start.x + 160, start.y + 80, { steps: 12 });
  await expect(page.locator('.tree-scene')).toHaveAttribute('transform', /translate/);
  await expect(svg).toHaveAttribute('viewBox', initial!);
  const dragged = (await svg.boundingBox())!;
  expect(dragged.x).toBeCloseTo(box.x, 2);
  expect(dragged.y).toBeCloseTo(box.y, 2);
  const beforeRelease = (await node.boundingBox())!;
  expect(beforeRelease.x).toBeGreaterThan(box.x);
  const groupsBefore = await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state.allocatedNodes);
  await page.mouse.up();
  await expect(page.locator('.tree-scene')).not.toHaveAttribute('transform', /translate/);
  const afterRelease = (await node.boundingBox())!;
  expect(Math.abs(afterRelease.x - beforeRelease.x)).toBeLessThan(1);
  expect(Math.abs(afterRelease.y - beforeRelease.y)).toBeLessThan(1);
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state.allocatedNodes)).toEqual(groupsBefore);

  // Starting on a node retains pointer capture, but must remain a pan rather
  // than an allocation click or a moving tooltip.
  await page.mouse.move(afterRelease.x + afterRelease.width / 2, afterRelease.y + afterRelease.height / 2);
  await page.mouse.down();
  await page.mouse.move(afterRelease.x + 80, afterRelease.y + 40, { steps: 12 });
  await expect(svg).toHaveClass(/is-dragging/);
  await expect(page.locator('.tree-tooltip')).toHaveCount(0);
  await page.mouse.up();
  await expect(page.locator('.tree-scene')).not.toHaveAttribute('transform', /translate/);
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state.allocatedNodes)).toEqual(groupsBefore);

  // A cancelled gesture must leave neither a displaced scene nor disabled hit testing.
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  await page.mouse.move(start.x + 90, start.y + 40, { steps: 4 });
  await svg.dispatchEvent('pointercancel', { pointerId: 1, isPrimary: true });
  await page.mouse.up();
  await expect(svg).not.toHaveClass(/is-dragging/);
  await expect(page.locator('.tree-scene')).not.toHaveAttribute('transform', /translate/);
});

test('a full Invoker tree needs explicit approval to apply future points', async ({ page }) => {
  const code = readFileSync(new URL('../../examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt', import.meta.url), 'utf8');
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await importBuild(page, code);
  await goTo(page, 'Tree');
  const count = page.locator('.tree-count');
  await expect(count).toContainText('Used 121 / 121 points', { timeout: 90_000 });
  await page.getByRole('button', { name: 'Plan passive upgrades', exact: true }).click();
  const mode = page.getByRole('combobox', { name: 'Plan', exact: true });
  const run = page.getByRole('button', { name: 'Find upgrade routes', exact: true });
  const savedNodes = () => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state.allocatedNodes as number[]);
  const before = await savedNodes();
  await expect(mode).toContainText('Spend new points');
  await expect(page.locator('.tree-planner-budget')).toContainText('No unspent points');
  await expect(page.getByText(/Automatic refunds are unavailable with weapon-set passives/)).toBeVisible();
  await expect(run).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Find jewel and passive plans', exact: true })).toBeDisabled();

  await mode.click();
  await page.getByRole('option', { name: 'Plan with future points', exact: true }).click();
  const points = page.getByRole('spinbutton', { name: 'Point budget', exact: true });
  await expect(points).toHaveValue('3');
  await run.click();
  const route = page.locator('.tree-planner-body > .tree-planner-results > li')
    .filter({ has: page.getByText('Extra passive points needed: 3', { exact: true }) }).first();
  await expect(route).toBeVisible({ timeout: 90_000 });
  expect(await savedNodes()).toEqual(before);
  await goTo(page, 'Overview');
  await goTo(page, 'Tree');
  await expect(mode).toContainText('Plan with future points');
  await expect(points).toHaveValue('3');
  const apply = route.getByRole('button', { name: 'Apply future plan', exact: true });
  const dialog = page.getByRole('alertdialog');
  await apply.click();
  await expect(dialog).toContainText('exceeds your current passive point budget by 3');
  expect(await savedNodes()).toEqual(before);
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  expect(await savedNodes()).toEqual(before);
  await apply.click();
  await dialog.getByRole('button', { name: 'Apply over budget', exact: true }).click();
  await expect(count).toContainText('Used 124 / 121 points');
  await expect(count).toContainText('Over budget by 3');
  await expect.poll(async () => (await savedNodes()).length).toBe(before.length + 3);
});

test('a respec preview fits both branches and can focus either one at every canvas width', async ({ page }) => {
  await caster(page);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await goTo(page, 'Tree');
  await page.getByRole('button', { name: 'Plan passive upgrades', exact: true }).click();
  await page.getByRole('combobox', { name: 'Plan', exact: true }).click();
  await page.getByRole('option', { name: 'Reallocate existing points', exact: true }).click();
  await page.getByRole('combobox', { name: 'Objective', exact: true }).click();
  await page.getByRole('option', { name: 'Max Effective HP', exact: true }).click();
  await page.getByRole('button', { name: 'Find upgrade routes', exact: true }).click();
  const route = page.locator('.tree-planner-body > .tree-planner-results > li').filter({ hasText: /Allocate 2.*Refund 3/ }).first();
  await expect(route).toBeVisible({ timeout: 90_000 });
  await expect(route.locator('.tree-plan-points')).toContainText('Save 1');
  await expect(page.locator('.tree-planner-summary')).not.toHaveAttribute('open', '');
  const trigger = route.getByRole('button', { name: 'Show on tree', exact: true });
  const tree = page.locator('.tree-canvas > svg');
  const controls = page.getByRole('group', { name: 'Route preview', exact: true });
  const before = await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state.allocatedNodes);
  const assertFit = async (scope: 'all' | 'add' | 'remove', count: number) => {
    await expect.poll(() => tree.evaluate((svg, { scope, count }) => {
      const markers = Array.from(svg.querySelectorAll<SVGGElement>(scope === 'all'
        ? '[data-route-node]' : `[data-route-node][data-change="${scope}"]`));
      const box = svg.getBoundingClientRect();
      const toolbar = svg.parentElement!.querySelector('.tree-route-preview')!.getBoundingClientRect();
      const points = markers.map(marker => new DOMPoint().matrixTransform(marker.getScreenCTM()!));
      return points.length === count && points.every(point => point.x > box.left + 10 && point.x < box.right - 10
        && point.y > toolbar.bottom + 10 && point.y < box.bottom - 10);
    }, { scope, count })).toBe(true);
  };
  for (const viewport of [{ width: 1920, height: 1040 }, { width: 1280, height: 760 }, { width: 390, height: 844 }]) {
    await page.setViewportSize(viewport);
    await trigger.click();
    await expect(controls).toBeInViewport({ ratio: 1 });
    await assertFit('all', 5);
    const overview = await tree.getAttribute('viewBox');
    const addButton = controls.getByRole('button', { name: 'Allocate 2', exact: true });
    if (viewport.width === 1920) {
      await tree.locator('.node-planned-remove').first().hover();
      await expect(page.locator('.tree-tooltip')).toBeVisible();
      await addButton.press('Enter');
      await expect(page.locator('.tree-tooltip')).toHaveCount(0);
    } else {
      await addButton.click();
    }
    await expect(tree).not.toHaveAttribute('viewBox', overview!);
    await assertFit('add', 2);
    const additions = await tree.getAttribute('viewBox');
    await controls.getByRole('button', { name: 'Refund 3', exact: true }).click();
    await expect(tree).not.toHaveAttribute('viewBox', additions!);
    await assertFit('remove', 3);
    if (viewport.width === 390) {
      await controls.getByRole('button', { name: 'Back to plan', exact: true }).click();
      await expect(trigger).toBeFocused();
      await trigger.press('Enter');
      await expect(controls).toBeFocused();
      await assertFit('all', 5);
    }
  }
  await controls.getByRole('button', { name: 'Clear preview', exact: true }).click();
  await expect(tree.locator('[data-route-node]')).toHaveCount(0);
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state.allocatedNodes)).toEqual(before);
});

test('editing passives automatically replans from the new tree and applying never spends points twice', async ({ page }) => {
  await caster(page);
  const before = await settledText(dps(page));
  await goTo(page, 'Tree');
  await page.locator('.tree-planner > .tree-planner-toggle').click();
  await page.getByRole('button', { name: 'Find upgrade routes', exact: true }).click();
  const plans = page.locator('.tree-planner-results > li');
  await expect(plans.first()).toBeVisible({ timeout: 30_000 });
  // Invoke the same node click handler without depending on tiny default-zoom hit boxes.
  await page.locator('circle[data-skill-id="4739"]').dispatchEvent('click');
  await expect(page.locator('circle[data-skill-id="4739"]')).not.toHaveClass(/node-allocated/);
  await expect(plans).toHaveCount(0);
  await expect(dps(page)).not.toHaveText(before);
  await expect(plans.first()).toBeVisible({ timeout: 30_000 });
  const currentDps = await settledNumber(dps(page));
  const evaluation = page.locator('.tree-planner-summary');
  await evaluation.locator(':scope > summary').click();
  const summary = await evaluation.innerText();
  const planningDps = Number(summary.match(/Combined DPS ([\d,.]+)/)![1].replaceAll(',', ''));
  expect(planningDps).toBeCloseTo(currentDps, 0);
  await plans.first().getByRole('button', { name: 'Apply plan', exact: true }).click();
  await expect(plans).toHaveCount(0);
  const applied = await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state.allocatedNodes);
  await expect(page.locator('.tree-planner-summary')).toBeVisible({ timeout: 30_000 });
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state.allocatedNodes)).toEqual(applied);

  await page.getByRole('combobox', { name: 'Plan', exact: true }).click();
  await page.getByRole('option', { name: 'Reallocate existing points', exact: true }).click();
  await expect(page.getByRole('spinbutton', { name: 'Maximum refunds' })).toHaveValue('3');
  await expect(page.locator('.tree-planner-summary')).toBeVisible({ timeout: 30_000 });
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state.allocatedNodes)).toEqual(applied);
  await page.locator('.tree-planner > .tree-planner-toggle').click();
  await page.locator('circle[data-skill-id="4739"]').dispatchEvent('click');
  await page.locator('.tree-planner > .tree-planner-toggle').click();
  await expect(page.locator('.tree-planner-summary')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Find upgrade routes', exact: true })).toBeEnabled();
  await expect(page.locator('.shell-alert')).toHaveCount(0);
});


test('support picker follows the selected group with shared engine compatibility', async ({ page }) => {
  await caster(page);
  await goTo(page, 'Skills');
  const addSkill = page.locator('.skills-toolbar input');
  await expect(addSkill).toBeEnabled();
  await addSkill.fill('Ice Shot');
  await page.getByRole('option', { name: /Ice Shot/ }).first().click();
  const groups = page.locator('.skill-group');
  await expect(groups).toHaveCount(2);
  // The list holds one row per group; the selected group's gems and its support picker sit in the panel beside it.
  const check = async (index: number, accepted: string, rejected: string) => {
    await expect(groups.nth(index).locator('.skill-group-title')).toHaveAttribute('aria-expanded', 'true');
    const picker = page.locator('.skill-group-picker input');
    await expect(picker).toBeEnabled();
    await picker.fill('Rapid');
    await expect(page.getByRole('option', { name: new RegExp(accepted) }).first()).toBeVisible();
    await expect(page.getByRole('option', { name: new RegExp(rejected) })).toHaveCount(0);
  };
  await check(1, 'Rapid Attacks', 'Rapid Casting');
  await groups.nth(0).locator('.skill-group-title').click();
  await check(0, 'Rapid Casting', 'Rapid Attacks');
  // Switch twice while compatibility is being refreshed. An old group's result
  // must never populate the newly opened picker.
  await groups.nth(1).locator('.skill-group-title').click();
  await groups.nth(0).locator('.skill-group-title').click();
  await check(0, 'Rapid Casting', 'Rapid Attacks');
});
