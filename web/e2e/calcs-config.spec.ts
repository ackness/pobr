import { expect, test, type Page } from '@playwright/test';
import type { CalculateBuildRequest, ConfigInputValue } from '../src/api/types';

type Probe = {
  requests: CalculateBuildRequest[]; metrics: number; attribution: number; holdNextDetail: boolean;
  release: () => void; edit: (key: string, value: ConfigInputValue | null) => void;
  focus: (id: string) => void; inputs: Record<string, ConfigInputValue>; busy: boolean;
  language: (lang: string) => void;
};
declare global { interface Window { calcsConfigProbe: Probe } }
const requests = (page: Page) => page.evaluate(() => window.calcsConfigProbe.requests);
const details = async (page: Page) => (await requests(page)).filter(request => request.breakdowns?.length);
const idle = (page: Page) => expect.poll(() => page.evaluate(() => window.calcsConfigProbe.busy)).toBe(false);
const input = (page: Page, key: string) => page.evaluate(key => window.calcsConfigProbe.inputs[key], key);

// The default production Playwright suite does not serve this development fixture.
test.beforeEach(async ({ page }, testInfo) => {
  test.skip(!testInfo.config.configFile?.endsWith('calcs-config.config.ts'), 'Run with --config e2e/calcs-config.config.ts');
  await page.goto('/e2e/fixtures/calcs-config.html');
  await expect(page.getByRole('button', { name: 'Import baseline' })).toBeEnabled();
  await page.getByRole('button', { name: 'Import baseline' }).click();
  await idle(page);
  await expect(page.locator('#breakdown-Life')).toBeVisible();
});

test('details load one actor and field, preserve search-independent selection and cache a repeat', async ({ page }) => {
  // Opening the page requests one useful detail; it never fetches every metric.
  expect((await requests(page)).every(request => (request.breakdowns?.length ?? 0) <= 1)).toBe(true);
  expect(await page.evaluate(() => window.calcsConfigProbe.metrics)).toBe(0);
  await page.locator('#breakdown-Life').click();
  await expect(page.locator('#calcs-detail .formula-final')).toContainText('1,234');
  const playerRequests = await details(page);
  expect(playerRequests.at(-1)).toMatchObject({ breakdowns: ['Life'], breakdown_actor: { kind: 'player' } });
  const commonCount = await page.locator('.calcs-metric-row').count();
  await page.getByRole('button', { name: 'All metrics', exact: true }).click();
  expect(await page.locator('.calcs-metric-row').count()).toBeGreaterThan(commonCount);
  await page.getByRole('button', { name: 'Common', exact: true }).click();
  await page.getByRole('searchbox').fill('FireResistMax');
  await expect(page.locator('#breakdown-FireResistMax')).toBeVisible();
  await page.getByRole('searchbox').fill('nonesuch');
  await expect(page.locator('#calcs-detail-heading')).toHaveText('Life');
  await page.evaluate(() => window.calcsConfigProbe.focus('Minion:0:Life'));
  await expect(page.getByRole('searchbox')).toHaveValue('');
  await expect(page.locator('#calcs-detail .formula-final')).toContainText('9,876');
  expect((await details(page)).at(-1)).toMatchObject({ breakdowns: ['Life'], breakdown_actor: { kind: 'minion', index: 0 } });
  await page.evaluate(() => window.calcsConfigProbe.focus('Life'));
  await expect(page.locator('#calcs-detail .formula-final')).toContainText('1,234');
  expect(await details(page)).toHaveLength(playerRequests.length + 1);
  await page.evaluate(() => window.calcsConfigProbe.focus('Minion:0:FireResistMax'));
  await expect(page.locator('#calcs-detail [role="status"]')).toBeVisible();
  expect(await details(page)).toHaveLength(playerRequests.length + 1);
});

test('late details cannot replace a newer calculation', async ({ page }) => {
  await page.locator('#breakdown-Life').click();
  await expect(page.locator('#calcs-detail .formula-final')).toContainText('1,234');
  const before = (await details(page)).length;
  await page.evaluate(() => { window.calcsConfigProbe.holdNextDetail = true; window.calcsConfigProbe.edit('probeLife', 1235); });
  await expect.poll(() => details(page).then(rows => rows.length)).toBe(before + 1);
  await page.evaluate(() => window.calcsConfigProbe.edit('probeLife', 7777));
  await idle(page);
  await expect(page.locator('#calcs-detail .formula-final')).toContainText('7,777');
  await page.evaluate(() => window.calcsConfigProbe.release());
  await expect(page.locator('#calcs-detail .formula-final')).toContainText('7,777');
});

test('overview, Full DPS and attribution remain separate and attribution is requested explicitly', async ({ page }) => {
  await page.getByRole('tab', { name: 'Skill metrics', exact: true }).click();
  await expect(page.locator('.skill-metrics-table')).toBeVisible();
  expect(await page.evaluate(() => window.calcsConfigProbe.metrics)).toBe(1);
  await expect(page.locator('.full-dps-table')).toHaveCount(0);
  await page.getByRole('tab', { name: 'Full DPS', exact: true }).click();
  await expect(page.locator('.full-dps-table')).toBeVisible();
  await expect(page.locator('.skill-metrics-table')).toHaveCount(0);
  await expect(page.locator('.full-dps-view')).toContainText('does not simulate a rotation');
  await page.getByRole('tab', { name: 'Source Attribution', exact: true }).click();
  expect(await page.evaluate(() => window.calcsConfigProbe.attribution)).toBe(0);
  await page.getByRole('button', { name: 'Run attribution', exact: true }).click();
  await expect(page.locator('.attribution-table')).toBeVisible();
  expect(await page.evaluate(() => window.calcsConfigProbe.attribution)).toBe(1);
});

test('config preserves unknown and saved-inapplicable values, commits numeric drafts and resets to imported zero', async ({ page }) => {
  await page.getByRole('button', { name: 'Open config' }).click();
  await idle(page);
  await expect(page.getByLabel('Unknown applicability', { exact: true })).toBeVisible();
  await expect(page.getByLabel('Unsaved inapplicable', { exact: true })).toHaveCount(0);
  const saved = page.locator('.config-item[data-config-var="TestSaved"]');
  await expect(saved).toContainText('Saved value kept visible');
  await expect(saved.getByRole('checkbox')).not.toBeChecked();
  const number = page.getByLabel('Test number', { exact: true });
  await number.fill('8.5');
  expect(await input(page, 'TestNumber')).toBe(0);
  await number.press('Enter');
  await idle(page);
  expect(await input(page, 'TestNumber')).toBe(8.5);
  await page.getByRole('button', { name: 'Restore imported/default value: Test number', exact: true }).click();
  await idle(page);
  await expect(number).toHaveValue('0');
  expect(await input(page, 'TestNumber')).toBe(0);
  const choice = page.getByLabel('Test choice', { exact: true });
  await choice.click();
  await page.getByRole('option', { name: 'Alternate', exact: true }).click();
  await idle(page);
  expect(await input(page, 'TestList')).toBe('alternate');
  await page.getByRole('button', { name: 'Show all options', exact: true }).click();
  await expect(page.getByLabel('Unsaved inapplicable', { exact: true })).toBeVisible();
  const last = (await requests(page)).at(-1)!;
  expect(last).toMatchObject({ config_context: true, breakdowns: [] });
  await page.getByRole('button', { name: 'Open calcs' }).click();
  await page.evaluate(() => window.calcsConfigProbe.edit('probeLife', 999));
  await idle(page);
  expect((await requests(page)).at(-1)!.config_context).toBeUndefined();
});

test('typed raw values remain distinct and party blocks are localized', async ({ page }) => {
  await page.getByRole('button', { name: 'Open config' }).click();
  await idle(page);
  await page.getByRole('tab', { name: 'Advanced values', exact: true }).click();
  await expect(page.getByLabel('Value type: rawText', { exact: true })).toHaveText('Text');
  await expect(page.getByLabel('Value type: rawZero', { exact: true })).toHaveText('Number');
  await expect(page.getByLabel('Value type: rawFalse', { exact: true })).toHaveText('Boolean');
  await page.getByLabel('rawText', { exact: true }).fill('20');
  await page.getByLabel('rawText', { exact: true }).press('Enter');
  await idle(page);
  expect(await input(page, 'rawText')).toBe('20');
  await page.getByLabel('Value type: rawText', { exact: true }).click();
  await page.getByRole('option', { name: 'Unset', exact: true }).click();
  await idle(page);
  expect(await input(page, 'rawText')).toBeUndefined();
  await page.getByRole('button', { name: 'Restore imported/default value: rawText', exact: true }).click();
  await idle(page);
  expect(await input(page, 'rawText')).toBe('0');
  await page.getByRole('tab', { name: 'Party', exact: true }).click();
  await page.getByRole('button', { name: 'Add member', exact: true }).click();
  await idle(page);
  await expect(page.locator('.party-editor')).toContainText('Enemy modifiers');
  await page.evaluate(() => window.calcsConfigProbe.language('zh-TW'));
  await expect(page.locator('.party-editor')).toContainText('敵人詞綴');
  await expect(page.locator('.party-editor')).not.toContainText('EnemyMods');
});

test('mobile detail navigation and config groups stay within the viewport', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator('#breakdown-Life').click();
  await expect(page.getByRole('button', { name: 'Back to metrics', exact: true })).toBeVisible();
  const tabs = await page.getByRole('tablist').boundingBox();
  const panel = await page.getByRole('tabpanel').boundingBox();
  expect(tabs!.y + tabs!.height).toBeLessThanOrEqual(panel!.y);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('calcs-mobile.png'), fullPage: true });
  await page.getByRole('button', { name: 'Open config' }).click();
  await idle(page);
  await expect(page.getByLabel('Test number', { exact: true })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('config-mobile.png'), fullPage: true });
});

test('desktop metric details stay beside the browser and config quick access focuses Radix controls', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1600, height: 1000 });
  await page.locator('#breakdown-Life').click();
  const browser = await page.locator('.calcs-metric-browser').boundingBox();
  const detail = await page.locator('.calcs-detail-pane').boundingBox();
  expect(browser!.x + browser!.width).toBeLessThan(detail!.x);
  await page.screenshot({ path: testInfo.outputPath('calcs-desktop.png'), fullPage: true });
  await page.getByRole('button', { name: 'Open config' }).click();
  await idle(page);
  await page.locator('.config-quick-disclosure > summary').click();
  await page.locator('.config-quick-entry').getByRole('button').click();
  await expect(page.locator('#config-conditionEnemyShocked')).toBeFocused();
  await page.screenshot({ path: testInfo.outputPath('config-desktop.png'), fullPage: true });
});
