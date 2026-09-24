import { expect, test, type Page } from '@playwright/test';

async function caster(page: Page) {
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: 85, class_name: 'Sorceress' },
    equipments: [{ inventoryId: 'Ring', x: 0, baseType: 'Sapphire Ring', name: 'Current Ring', frameType: 2,
      explicitMods: ['40% increased Fire Damage', '+80 to maximum Life'] }],
    talent_tree: { hashes: [54447, 4739, 22419, 18407], quest_stats: [] }, jewel_data: '[]',
    skills: [{ baseType: 'Fireball', support: false, properties: [{ type: 5, values: [['16', 0]] }] }],
  } }));
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticGuidanceFixture');
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 30_000 });
}
const nav = (page: Page, name: string) => page.getByRole('navigation', { name: 'Main navigation' }).getByRole('button', { name, exact: true });
const dps = (page: Page) => page.locator('.stat-row').filter({ hasText: 'Total DPS' }).locator('dd');

test('current market Sum is a reference; complete replacement detects a downgrade before applying', async ({ page }) => {
  await caster(page);
  const before = await dps(page).innerText();
  await nav(page, 'Upgrades').click();
  await page.locator('.trade-position').filter({ hasText: /^Ring 1/ }).click();
  await page.getByRole('button', { name: 'Calculate affix scores', exact: true }).click();
  const reference = page.locator('.upgrade-score-reference');
  await expect(reference).toBeVisible();
  await expect(reference).toContainText('Current item · market Sum');
  const sum = Number((await reference.locator('div > strong').first().innerText()).replaceAll(',', ''));
  const href = await page.locator('.trade-market-link').first().getAttribute('href');
  const query = JSON.parse(new URL(href!).searchParams.get('q')!);
  expect(sum).toBeGreaterThan(0);
  expect(query.query.stats[0].value.min).toBeCloseTo(sum, 3);
  await page.getByRole('textbox', { name: 'Complete item text' }).fill('Rarity: NORMAL\nSapphire Ring');
  await page.getByRole('button', { name: 'Calculate replacement' }).click();
  await expect(page.locator('.upgrade-replacement-result')).toContainText('DPS decreases after this replacement');
  await expect(dps(page)).toHaveText(before);
  const preview = await page.locator('.upgrade-replacement-result tbody tr').first().locator('td').nth(1).innerText();
  await page.getByRole('button', { name: 'Apply to build · Ring 1', exact: true }).click();
  await expect(dps(page)).not.toHaveText(before);
  expect(Number((await dps(page).innerText()).replaceAll(',', ''))).toBeCloseTo(Number(preview.replaceAll(',', '')), 0);
  await expect(reference).toHaveCount(0);
  await page.getByRole('textbox', { name: 'Complete item text' }).fill('Rarity: NORMAL\nTwin Bow');
  await page.getByRole('button', { name: 'Calculate replacement' }).click();
  await expect(page.getByRole('alert')).toContainText('No compatible equipped position');
  await expect(page.getByRole('button', { name: /Apply to build/ })).toHaveCount(0);
});

test('shared goals link automatic supports and connected passive plans with real apply', async ({ page }) => {
  await caster(page);
  await nav(page, 'Upgrades').click();
  await page.getByRole('checkbox', { name: 'Prioritize capped elemental resistances' }).check();
  await page.locator('.upgrade-paths > button').nth(1).click();
  await expect(page.locator('.gem-optimizer-body')).toBeVisible();
  await expect(page.getByRole('checkbox', { name: 'Prioritize capped elemental resistances' })).toBeChecked();
  await expect.poll(async () => Number(await page.locator('.support-pool-summary strong').innerText())).toBeGreaterThan(20);
  await page.getByRole('button', { name: 'Calculate support combinations', exact: true }).click();
  const support = page.locator('.support-plan').first();
  await expect(support).toBeVisible({ timeout: 30_000 });
  await expect(support.locator('.support-plan-metrics')).toContainText('EHP');
  const before = await dps(page).innerText();
  await support.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(dps(page)).not.toHaveText(before);
  await nav(page, 'Upgrades').click();
  await page.locator('.trade-position').filter({ hasText: 'Skill and support gems' }).click();
  await page.getByRole('button', { name: 'Calculate affix scores', exact: true }).click();
  await expect(page.locator('.trade-gems')).toBeVisible();
  for (const card of await page.locator('.trade-gem-card').filter({ hasText: 'Skill adjustment' }).all()) {
    await expect(card.getByRole('link')).toHaveCount(0);
  }
  await page.locator('.upgrade-paths > button').nth(2).click();
  await expect(page.locator('.tree-planner-body')).toBeVisible();
  await expect(page.getByRole('checkbox', { name: 'Prioritize capped elemental resistances' })).toBeChecked();
  await page.locator('.tree-planner-run').click();
  const route = page.locator('.tree-planner-results > li').first();
  await expect(route).toBeVisible({ timeout: 30_000 });
  await route.getByRole('button', { name: 'Show on tree' }).click();
  await expect(page.locator('.node-planned-add').first()).toBeVisible();
  const treeBefore = await dps(page).innerText();
  await route.getByRole('button', { name: 'Apply plan' }).click();
  await expect(page.locator('.tree-planner-results > li')).toHaveCount(0);
  await expect(dps(page)).not.toHaveText(treeBefore);
  await expect(page.locator('.calc-error')).toHaveCount(0);
});

test('support exclusions persist, invalidate old plans and constrain real-WASM recommendations', async ({ page }) => {
  const openOptimizer = async () => {
    await nav(page, 'Skills').click();
    await expect(page.locator('.skills-toolbar input')).toBeEnabled();
    const group = page.locator('.skill-group-title').first();
    if (await group.getAttribute('aria-expanded') === 'false') await group.click();
    const toggle = page.locator('.gem-optimizer-toggle');
    if (await toggle.getAttribute('aria-expanded') === 'false') await toggle.click();
  };
  await caster(page);
  const baseline = await dps(page).innerText();
  await nav(page, 'Upgrades').click();
  await page.locator('.upgrade-paths > button').nth(1).click();
  const run = page.getByRole('button', { name: 'Calculate support combinations', exact: true });
  const count = page.locator('.support-pool-summary strong');
  await expect.poll(async () => Number(await count.innerText())).toBeGreaterThan(20);
  const candidates = Number(await count.innerText());
  await run.click();
  await expect(page.locator('.support-plan').first()).toBeVisible({ timeout: 30_000 });
  const gem = page.locator('.support-plan-gems > .opt-chip').first();
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
  for (const width of [320, 768, 1440]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.locator('main').evaluate(element => element.scrollWidth - element.clientWidth);
    expect(overflow, `Support exclusions and results must fit ${width}px`).toBeLessThanOrEqual(1);
  }

  await nav(page, 'Items').click();
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
    const key = Object.keys(localStorage).find(key => key.startsWith('pobr-support-exclusions:'))!;
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
  await nav(page, 'Tree').click();
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
  const groupsBefore = await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!).state.allocatedNodes);
  await page.mouse.up();
  await expect(page.locator('.tree-scene')).not.toHaveAttribute('transform', /translate/);
  const afterRelease = (await node.boundingBox())!;
  expect(Math.abs(afterRelease.x - beforeRelease.x)).toBeLessThan(1);
  expect(Math.abs(afterRelease.y - beforeRelease.y)).toBeLessThan(1);
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!).state.allocatedNodes)).toEqual(groupsBefore);

  // Starting on a node retains pointer capture, but must remain a pan rather
  // than an allocation click or a moving tooltip.
  await page.mouse.move(afterRelease.x + afterRelease.width / 2, afterRelease.y + afterRelease.height / 2);
  await page.mouse.down();
  await page.mouse.move(afterRelease.x + 80, afterRelease.y + 40, { steps: 12 });
  await expect(svg).toHaveClass(/is-dragging/);
  await expect(page.locator('.tree-tooltip')).toHaveCount(0);
  await page.mouse.up();
  await expect(page.locator('.tree-scene')).not.toHaveAttribute('transform', /translate/);
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!).state.allocatedNodes)).toEqual(groupsBefore);

  // A cancelled gesture must leave neither a displaced scene nor disabled hit testing.
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  await page.mouse.move(start.x + 90, start.y + 40, { steps: 4 });
  await svg.dispatchEvent('pointercancel', { pointerId: 1, isPrimary: true });
  await page.mouse.up();
  await expect(svg).not.toHaveClass(/is-dragging/);
  await expect(page.locator('.tree-scene')).not.toHaveAttribute('transform', /translate/);
});

test('editing passives automatically replans from the new tree and applying never spends points twice', async ({ page }) => {
  await caster(page);
  const before = await dps(page).innerText();
  await nav(page, 'Tree').click();
  await page.locator('.tree-planner-toggle').click();
  await page.locator('.tree-planner-run').click();
  const plans = page.locator('.tree-planner-results > li');
  await expect(plans.first()).toBeVisible({ timeout: 30_000 });
  // Invoke the same node click handler without depending on tiny default-zoom hit boxes.
  await page.locator('circle[data-skill-id="4739"]').dispatchEvent('click');
  await expect(page.locator('circle[data-skill-id="4739"]')).not.toHaveClass(/node-allocated/);
  await expect(plans).toHaveCount(0);
  await expect(dps(page)).not.toHaveText(before);
  await expect(plans.first()).toBeVisible({ timeout: 30_000 });
  const currentDps = Number((await dps(page).innerText()).replaceAll(',', ''));
  const summary = await page.locator('.tree-planner-summary').innerText();
  const planningDps = Number(summary.match(/DPS ([\d,.]+)/)![1].replaceAll(',', ''));
  expect(planningDps).toBeCloseTo(currentDps, 0);
  await plans.first().getByRole('button', { name: 'Apply plan', exact: true }).click();
  await expect(plans).toHaveCount(0);
  const applied = await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!).state.allocatedNodes);
  await expect(page.locator('.tree-planner-summary')).toBeVisible({ timeout: 30_000 });
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!).state.allocatedNodes)).toEqual(applied);

  await page.getByRole('button', { name: 'Plan', exact: true }).click();
  await page.getByRole('option', { name: 'Reallocate existing points', exact: true }).click();
  await expect(page.getByRole('spinbutton', { name: 'Maximum refunds' })).toHaveValue('3');
  await expect(page.locator('.tree-planner-summary')).toBeVisible({ timeout: 30_000 });
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!).state.allocatedNodes)).toEqual(applied);
  await page.locator('.tree-planner-toggle').click();
  await page.locator('circle[data-skill-id="4739"]').dispatchEvent('click');
  await page.locator('.tree-planner-toggle').click();
  await expect(page.locator('.tree-planner-summary')).toHaveCount(0);
  await expect(page.locator('.tree-planner-run')).toBeEnabled();
  await expect(page.locator('.calc-error')).toHaveCount(0);
});


test('support picker follows the selected group with shared engine compatibility', async ({ page }) => {
  await caster(page);
  await nav(page, 'Skills').click();
  const addSkill = page.locator('.skills-toolbar input');
  await expect(addSkill).toBeEnabled();
  await addSkill.fill('Ice Shot');
  await page.locator('.skills-toolbar').getByRole('option', { name: /Ice Shot/ }).first().click();
  const groups = page.locator('.skill-group');
  await expect(groups).toHaveCount(2);
  const check = async (index: number, accepted: string, rejected: string) => {
    const picker = groups.nth(index).locator('.skill-group-picker input');
    await expect(picker).toBeEnabled();
    await picker.fill('Rapid');
    await expect(groups.nth(index).getByRole('option', { name: new RegExp(accepted) }).first()).toBeVisible();
    await expect(groups.nth(index).getByRole('option', { name: new RegExp(rejected) })).toHaveCount(0);
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
