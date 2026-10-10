import { expect, test, type Page } from '@playwright/test';
import { deflateSync } from 'node:zlib';
import { installTradeCatalogFixture } from './tradeCatalogFixture';
import { categoryAffixPool } from '../src/lib/tradeOptimizer';

declare global {
  interface Window {
    resumedTradePositions: string[];
  }
}

const code = (rings = 1) => deflateSync(`<PathOfBuilding2>
  <Build level="70" className="Witch"/>
  <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
  <Skills/>
  <Items activeItemSet="1">
    <Item id="1">Rarity: RARE
Reference Ring
Sapphire Ring
Implicits: 0
+50 to maximum Life</Item>
    ${rings === 2 ? `<Item id="2">Rarity: RARE
Second Ring
Sapphire Ring
Implicits: 0
+75 to maximum Life</Item>` : ''}
    <ItemSet id="1"><Slot name="Ring 1" itemId="1"/>${rings === 2 ? '<Slot name="Ring 2" itemId="2"/>' : ''}</ItemSet>
  </Items>
</PathOfBuilding2>`).toString('base64url');

const nav = (page: Page, name: string) =>
  page.getByRole('navigation', { name: 'Main navigation' }).getByRole('button', { name, exact: true });

async function openTrade(page: Page, rings = 1) {
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await installTradeCatalogFixture(page, catalog => {
    catalog.mods = categoryAffixPool(catalog, 'accessory.ring', 88, 70).filter(mod =>
      mod.lines.some(line => /to maximum Life|to Fire Resistance/.test(line))).slice(0, 10);
    catalog.search_mods = [];
  });
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code(rings));
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible();
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await nav(page, 'Upgrades').click();
  await page.getByRole('button', { name: 'Max Life', exact: true }).click();
  await page.locator('.trade-position').filter({ hasText: /^Ring 1/ }).click();
  await expect(page.getByRole('button', { name: 'Calculate affix scores', exact: true })).toBeEnabled();
}

async function completeAll(page: Page) {
  await page.getByRole('button', { name: 'Analyze all positions', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Reanalyze all positions', exact: true })).toBeEnabled({ timeout: 90_000 });
  await expect(page.locator('.upgrade-score-reference')).toBeVisible();
}

async function expectCleared(page: Page) {
  await expect(page.locator('.upgrade-score-reference')).toHaveCount(0);
  await expect(page.locator('.trade-score-table')).toHaveCount(0);
  await expect(page.locator('.trade-market-link')).toHaveCount(0);
  await expect(page.locator('.trade-position-done')).toHaveCount(0);
  await expect(page.locator('.trade-priorities .ui-badge')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Analyze all positions', exact: true })).toBeEnabled();
}

async function pauseScoringClock(page: Page) {
  // Freeze only scheduling, after initialization; every variant still uses real WASM.
  await page.clock.install({ time: new Date('2026-01-01T00:00:00Z') });
  await page.clock.pauseAt(new Date('2026-01-01T00:00:01Z'));
}

test('completed trade analysis and Ring 1 selection survive tabs and market edits but reset for a new goal', async ({ page }) => {
  await openTrade(page);
  await page.getByRole('button', { name: 'Calculate affix scores', exact: true }).click();
  await expect(page.locator('.upgrade-score-reference')).toBeVisible({ timeout: 90_000 });
  const reference = page.locator('.upgrade-score-reference');
  const score = await reference.locator('div > strong').first().innerText();
  const scores = await page.locator('.trade-score-table').innerText();
  const link = page.locator('.trade-market-link').first();
  const originalHref = (await link.getAttribute('href'))!;
  const query = JSON.parse(new URL(originalHref).searchParams.get('q')!);
  const numericScore = Number(score.replaceAll(',', ''));
  expect(numericScore).toBeGreaterThan(0);
  expect(query.query.stats[0].value.min).toBeCloseTo(numericScore, 3);
  await expect(page.getByRole('button', { name: 'Analyze all positions', exact: true })).toBeEnabled();

  await nav(page, 'Calcs').click();
  await expect(page.locator('.trade-page')).toBeHidden();
  await expect(page.getByRole('button', { name: 'Analyze all positions', exact: true })).toHaveCount(0);
  await expect(page.getByRole('spinbutton', { name: 'Max price', exact: true })).toHaveCount(0);
  await expect(page.getByRole('textbox', { name: 'Complete item text', exact: true })).toHaveCount(0);
  await nav(page, 'Upgrades').click();
  await expect(page.locator('.trade-position[aria-pressed="true"] strong')).toHaveText('Ring 1');
  await expect(reference.locator('div > strong').first()).toHaveText(score);
  expect(await page.locator('.trade-score-table').innerText()).toBe(scores);
  await expect(link).toHaveAttribute('href', originalHref);
  await expect(page.getByRole('button', { name: 'Analyze all positions', exact: true })).toBeEnabled();
  await expect(page.locator('.trade-progress')).toHaveCount(0);

  await completeAll(page);
  await expect(page.locator('.trade-priorities .ui-badge')).toContainText('1 / 1');
  const allScore = await reference.locator('div > strong').first().innerText();
  const allScores = await page.locator('.trade-score-table').innerText();

  await page.getByRole('spinbutton', { name: 'Max price' }).fill('50');
  const marketHref = (await link.getAttribute('href'))!;
  expect(JSON.parse(new URL(marketHref).searchParams.get('q')!).query.filters.trade_filters.filters.price.max).toBe(50);
  await nav(page, 'Build').click();
  await nav(page, 'Upgrades').click();
  await expect(reference.locator('div > strong').first()).toHaveText(allScore);
  expect(await page.locator('.trade-score-table').innerText()).toBe(allScores);
  await expect(link).toHaveAttribute('href', marketHref);
  await expect(page.locator('.trade-position[aria-pressed="true"] strong')).toHaveText('Ring 1');
  await expect(page.locator('.trade-priorities .ui-badge')).toContainText('1 / 1');

  await page.getByRole('button', { name: 'Max total DPS', exact: true }).click();
  await expectCleared(page);
});

test('completed scores invalidate for build and shared-goal edits while Upgrades is hidden', async ({ page }) => {
  await openTrade(page);
  await completeAll(page);
  await nav(page, 'Build').click();
  await page.getByLabel('Level', { exact: true }).fill('71');
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await expect(page.locator('.trade-page')).toHaveCount(1);
  await expect(page.locator('.upgrade-score-reference')).toHaveCount(0);
  await nav(page, 'Upgrades').click();
  await expectCleared(page);
  await expect(page.locator('.trade-position[aria-pressed="true"] strong')).toHaveText('Ring 1');

  await completeAll(page);
  await nav(page, 'Tree').click();
  await page.locator('.tree-planner-toggle').click();
  await page.getByRole('checkbox', { name: 'Prioritize capped elemental resistances', exact: true }).check();
  await expect(page.locator('.trade-page')).toHaveCount(1);
  await expect(page.locator('.upgrade-score-reference')).toHaveCount(0);
  await nav(page, 'Upgrades').click();
  await expectCleared(page);
  await expect(page.getByRole('checkbox', { name: 'Prioritize capped elemental resistances', exact: true })).toBeChecked();
});

test('switching stages discards completed scores and position selection even for an identical copied build', async ({ page }) => {
  await openTrade(page);
  await completeAll(page);
  await nav(page, 'Build').click();
  const originalStageName = await page.locator('.workspace-current-stage').innerText();
  await page.getByRole('button', { name: 'Copy current stage', exact: true }).click();
  await page.getByRole('textbox', { name: 'Name', exact: true }).fill('Copied stage');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await nav(page, 'Upgrades').click();
  await expectCleared(page);
  await expect(page.locator('.trade-position[aria-pressed="true"] strong')).toHaveText('Main Hand');

  await nav(page, 'Build').click();
  await page.getByRole('combobox', { name: 'Stage', exact: true }).selectOption({ label: originalStageName });
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await nav(page, 'Upgrades').click();
  await expectCleared(page);
  await expect(page.locator('.trade-position[aria-pressed="true"] strong')).toHaveText('Main Hand');
});

test('leaving all-position analysis stops hidden work and resumes only the unfinished position', async ({ page }) => {
  await openTrade(page, 2);
  await pauseScoringClock(page);
  await page.getByRole('button', { name: 'Analyze all positions', exact: true }).click({ force: true });
  const completed = page.locator('.trade-priorities .ui-badge');
  await expect.poll(async () => {
    await page.clock.runFor(1);
    return completed.textContent();
  }, { timeout: 90_000, intervals: [10] }).toContain('1 / 2');
  await expect(page.locator('.trade-progress > span')).toContainText('Ring 2');
  const reference = page.locator('.upgrade-score-reference');
  const score = await reference.locator('div > strong').first().innerText();
  const scores = await page.locator('.trade-score-table').innerText();
  const href = (await page.locator('.trade-market-link').first().getAttribute('href'))!;

  await nav(page, 'Calcs').click({ force: true });
  await expect(page.getByRole('button', { name: 'Resume remaining positions', exact: true })).toHaveCount(0);
  // Drain the pending scoring yield: abort must prevent hidden completion/publication.
  await page.clock.runFor(1000);
  await expect(completed).toContainText('1 / 2');
  await expect(page.locator('.trade-position-done')).toHaveCount(1);
  await expect(page.locator('.trade-progress')).toHaveCount(0);
  await nav(page, 'Upgrades').click({ force: true });
  await expect(page.getByRole('button', { name: 'Resume remaining positions', exact: true })).toBeEnabled();
  await expect(page.getByRole('status').filter({ hasText: 'Analysis stopped.' })).toBeVisible();
  await expect(page.locator('.trade-position[aria-pressed="true"] strong')).toHaveText('Ring 1');
  await expect(reference.locator('div > strong').first()).toHaveText(score);
  expect(await page.locator('.trade-score-table').innerText()).toBe(scores);
  await expect(page.locator('.trade-market-link').first()).toHaveAttribute('href', href);

  // Observe consumer-visible scoring transitions, not source text or a fake backend.
  await page.evaluate(() => {
    const positions: string[] = [];
    window.resumedTradePositions = positions;
    new MutationObserver(() => {
      const position = document.querySelector('.trade-progress > span')?.textContent?.match(/Ring [12]/)?.[0];
      if (position && positions.at(-1) !== position) positions.push(position);
    }).observe(document.querySelector('.trade-page')!, { childList: true, characterData: true, subtree: true });
  });
  await page.getByRole('button', { name: 'Resume remaining positions', exact: true }).click({ force: true });
  await expect(page.locator('.trade-progress > span')).toContainText('Ring 2');
  await page.clock.resume();
  await expect(page.getByRole('button', { name: 'Reanalyze all positions', exact: true })).toBeEnabled({ timeout: 90_000 });
  await expect(completed).toContainText('2 / 2');
  await expect(page.locator('.trade-position-done')).toHaveCount(2);
  expect(await page.evaluate(() => window.resumedTradePositions)).toEqual(['Ring 2']);
  await expect(reference.locator('div > strong').first()).toHaveText(score);
  expect(await page.locator('.trade-score-table').innerText()).toBe(scores);
  await expect(page.locator('.trade-market-link').first()).toHaveAttribute('href', href);
});

for (const edit of ['build', 'goal'] as const) {
  test(`an aborted analysis cannot restore scores after a hidden ${edit} edit`, async ({ page }) => {
    await openTrade(page);
    await pauseScoringClock(page);
    await page.getByRole('button', { name: 'Analyze all positions', exact: true }).click({ force: true });
    await expect(page.locator('.trade-progress > span')).toContainText('Ring 1');
    if (edit === 'build') {
      await nav(page, 'Build').click({ force: true });
      await page.getByLabel('Level', { exact: true }).fill('71');
    } else {
      await nav(page, 'Tree').click({ force: true });
      await page.locator('.tree-planner-toggle').click({ force: true });
      await page.getByRole('checkbox', { name: 'Prioritize capped elemental resistances', exact: true }).check({ force: true });
    }
    await page.clock.runFor(1000);
    await expect(page.locator('.trade-page')).toHaveCount(1);
    await expect(page.locator('.upgrade-score-reference')).toHaveCount(0);
    await expect(page.locator('.trade-position-done')).toHaveCount(0);
    await nav(page, 'Upgrades').click({ force: true });
    await expectCleared(page);
    await expect(page.getByRole('status').filter({ hasText: 'Analysis stopped.' })).toHaveCount(0);
    await page.clock.resume();
  });
}
