import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';

const code = readFileSync(new URL('../../examples/demo-bd-test/builds/ranger-pathfinder-ice-shot/code.txt', import.meta.url), 'utf8').trim();
const secondCode = readFileSync(new URL('../../examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt', import.meta.url), 'utf8').trim();
const nav = (page: import('@playwright/test').Page, name: string) => page.getByRole('navigation', { name: 'Main navigation' }).getByRole('button', { name, exact: true });

async function importBuild(page: import('@playwright/test').Page) {
  await page.goto('/');
  await expect(page.getByRole('textbox', { name: 'Build code' })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill(code);
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 60_000 });
  await nav(page, 'Build references').click();
  await expect(page.locator('.guidance-result')).toHaveCount(17, { timeout: 30_000 });
}

test('real imported build ranks its reference first, compares without editing, and evaluates an item', async ({ page }) => {
  await importBuild(page);
  const save = await page.evaluate(() => localStorage.getItem('pobr-build-state'));
  expect(save).not.toBeNull();
  const first = page.locator('.guidance-result').first();
  await expect(first).toContainText('Ice Shot');
  await expect(first).toContainText('Pathfinder');
  await expect(first.locator('.guidance-score strong')).toHaveText('100');
  const link = page.getByRole('link', { name: 'Search current skill' });
  const url = new URL((await link.getAttribute('href'))!);
  expect(url.searchParams.get('skills')).toBe('Ice Shot');
  expect(url.searchParams.get('class')).toBe('Pathfinder');
  await first.getByRole('button', { name: 'Compare details' }).click();
  await expect(page.locator('.guidance-table')).toContainText('Ice Shot');
  await expect(page.locator('.guidance-equipment')).toContainText('Reference');
  await expect(page.locator('.guidance-passives')).toBeVisible();
  expect(await page.evaluate(() => localStorage.getItem('pobr-build-state'))).toBe(save);
  await page.getByRole('textbox', { name: 'Filter by skill, class or unique' }).fill('no-such-build-example');
  await expect(page.locator('.guidance-result')).toHaveCount(0);
  await expect(page.locator('.guidance-empty')).toBeVisible();
  await page.getByRole('textbox', { name: 'Reference PoB2 code' }).fill(secondCode);
  await page.getByRole('button', { name: 'Open reference', exact: true }).click();
  await expect(page.locator('#reference-heading')).toContainText('Frost Bomb');
  expect(await page.evaluate(() => localStorage.getItem('pobr-build-state'))).toBe(save);
  await page.locator('.guidance-equipment').getByRole('button', { name: 'Evaluate on my character' }).first().click();
  await expect(page.locator('#replacement-text')).not.toHaveValue('');
  await expect(page.locator('.upgrade-replacement-result, .upgrade-item-check [role="alert"]')).toBeVisible({ timeout: 30_000 });
  expect(await page.evaluate(() => localStorage.getItem('pobr-build-state'))).toBe(save);
});

test('sample failure leaves manual comparison usable and never mutates a build', async ({ page }) => {
  await page.route('**/build-references/index.json', route => route.fulfill({ status: 503, body: '{}' }));
  await page.goto('/');
  await expect(page.getByRole('textbox', { name: 'Build code' })).toBeVisible({ timeout: 90_000 });
  await nav(page, 'Build references').click();
  await expect(page.getByRole('alert')).toContainText('Reference samples could not be loaded');
  await page.getByRole('textbox', { name: 'Reference PoB2 code' }).fill('invalid-code');
  await page.getByRole('button', { name: 'Open reference', exact: true }).click();
  await expect(page.locator('.guidance-paste [role="alert"]')).toBeVisible();
  await page.getByRole('textbox', { name: 'Reference PoB2 code' }).fill(secondCode);
  await page.getByRole('button', { name: 'Open reference', exact: true }).click();
  await expect(page.locator('#reference-heading')).toContainText('Frost Bomb');
  await expect(page.locator('.guidance-result').first().locator('.guidance-score strong')).toHaveText('0');
});

for (const width of [320, 768, 1024, 1440]) {
  test(`reference details fit ${width}px in Chinese`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.addInitScript(() => localStorage.setItem('pobr-lang', 'zh-CN'));
    await page.goto('/');
    await expect(page.getByRole('textbox', { name: 'Build code' })).toBeVisible({ timeout: 90_000 });
    await page.getByRole('textbox', { name: 'Build code' }).fill(code);
    await page.locator('.import-submit').click();
    await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 60_000 });
    await nav(page, '流派参考').click();
    await expect(page.locator('.guidance-result')).toHaveCount(17);
    await page.locator('.guidance-result').first().getByRole('button', { name: '对照细节' }).click();
    await expect(page.locator('.guidance-detail')).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth)).toBeLessThanOrEqual(1);
    expect(await page.locator('main').evaluate(element => element.scrollWidth - element.clientWidth)).toBeLessThanOrEqual(1);
    await page.screenshot({ path: `e2e/screenshots/guidance-${width}.png` });
  });
}
