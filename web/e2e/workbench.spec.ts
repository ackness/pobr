import { expect, test, type Page } from '@playwright/test';
import { deflateSync } from 'node:zlib';
import { installTradeCatalogFixture } from './tradeCatalogFixture';

// Same minimal Witch build as workspace.spec.ts; the engine calculates it for real.
const code = deflateSync('<PathOfBuilding2><Build level="60" className="Witch" mainSocketGroup="1"/><Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree><Skills activeSkillSet="1"><SkillSet id="1"><Skill enabled="true"><Gem skillId="FireballPlayer" level="12" quality="0" enabled="true"/></Skill></SkillSet></Skills><Items/><Config><Input name="enemyLevel" number="82"/></Config></PathOfBuilding2>').toString('base64url');

async function importBuild(page: Page) {
  await page.addInitScript(() => {
    if (!localStorage.getItem('pobr-v2-preferences')) localStorage.setItem('pobr-v2-preferences', JSON.stringify({ lang: 'en-US', theme: 'dark', font: 'system', fontSize: 16, density: 'comfortable' }));
  });
  await page.goto('/build');
  await expect(page.getByRole('textbox', { name: 'Build code', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();
  await expect(page.locator('#ov-title')).toBeVisible();
  await expect(page.locator('.shell-save-state')).toHaveText('Saved locally');
}

test('a number in the results rail opens the calculation behind it, and it can be closed', async ({ page }) => {
  await importBuild(page);
  // An exact label: `hasText` is a substring match and would pick "Enemy Life Equivalent".
  await page.locator('.rail-row.is-link').filter({ has: page.locator('.rail-label', { hasText: /^Life$/ }) }).first().click();
  await expect(page.locator('.insp h2')).toHaveText('Life');
  await expect(page.locator('.insp .formula-final, .insp .breakdown-summary').first()).toBeVisible({ timeout: 30_000 });
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(page.locator('.insp')).toHaveCount(0);
});

test('Ctrl+K finds a stat by name and opens its calculation', async ({ page }) => {
  await importBuild(page);
  await page.keyboard.press('Control+k');
  const palette = page.getByRole('dialog', { name: 'Search and commands' });
  await expect(palette).toBeVisible();
  await page.getByPlaceholder(/Search pages/).fill('Life');
  await page.keyboard.press('Enter');
  await expect(palette).toHaveCount(0);
  await expect(page.locator('.insp')).toBeVisible();
});

test('the palette switches the enemy and pages without leaving the keyboard', async ({ page }) => {
  await importBuild(page);
  await page.keyboard.press('Control+k');
  await page.getByPlaceholder(/Search pages/).fill('Boss');
  await page.keyboard.press('Enter');
  await expect(page.getByRole('combobox', { name: 'Enemy tier' })).toContainText(/boss/i);
  await page.keyboard.press('Control+k');
  await page.getByPlaceholder(/Search pages/).fill('Items');
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(/\/items$/);
});

test('number keys, [ and ? work outside text fields and stay out of the way inside them', async ({ page }) => {
  await importBuild(page);
  await page.locator('body').click({ position: { x: 5, y: 5 } });
  await page.keyboard.press('3');
  await expect(page).toHaveURL(/\/items$/);
  await page.keyboard.press('[');
  await expect(page.locator('.shell-body[data-rail]')).toHaveCount(1);
  await page.keyboard.press('[');
  await expect(page.locator('.shell-body[data-rail]')).toHaveCount(0);
  await page.keyboard.press('?');
  await expect(page.getByRole('dialog', { name: 'Keyboard shortcuts' })).toBeVisible();
  await page.keyboard.press('Escape');
  await page.goto('/build');
  const box = page.getByRole('textbox', { name: 'Build code', exact: true });
  await box.click(); await page.keyboard.type('3');
  await expect(box).toHaveValue('3');
  await expect(page).toHaveURL(/\/build$/);
});

test('Ctrl+Z and Ctrl+Shift+Z walk back and forward through edits', async ({ page }) => {
  await importBuild(page);
  const enemy = page.getByRole('combobox', { name: 'Enemy tier' });
  await expect(enemy).toContainText(/pinnacle/i); // the engine's default when a build sets none
  await page.keyboard.press('Control+k');
  await page.getByPlaceholder(/Search pages/).fill('Boss');
  await page.keyboard.press('Enter');
  await expect(enemy).toContainText(/^Boss$|Standard Boss/i);
  await page.locator('body').click({ position: { x: 5, y: 5 } });
  await page.keyboard.press('Control+z');
  await expect(enemy).toContainText(/pinnacle/i);
  await page.keyboard.press('Control+Shift+z');
  await expect(enemy).not.toContainText(/pinnacle/i);
});

test('a stat pinned from the inspector stays at the top of the results panel', async ({ page }) => {
  await importBuild(page);
  await page.locator('.rail-row.is-link').filter({ has: page.locator('.rail-label', { hasText: /^Life$/ }) }).first().click();
  await page.getByRole('button', { name: 'Pin to the results panel', exact: true }).click();
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(page.locator('.rail-pinned .rail-label', { hasText: /^Life$/ })).toBeVisible();
  await page.reload();
  await expect(page.locator('.rail-pinned .rail-label', { hasText: /^Life$/ })).toBeVisible({ timeout: 90_000 });
});

test('hovering a library item previews its effect without changing the build', async ({ page }) => {
  await importBuild(page);
  await page.goto('/items');
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 90_000 });
  // Put a ring on one hand and save it to the library, then look at it from the other ring slot.
  await page.locator('.doll-cell[data-slot="ring1"] .doll-slot').click();
  // An empty slot opens its editor straight away, already holding a template item.
  await page.getByRole('button', { name: 'Save & recalculate', exact: true }).click();
  await page.getByRole('button', { name: 'Save to library', exact: true }).click();
  await expect(page.locator('.shell-save-state')).toHaveText('Saved locally');
  await page.locator('.doll-cell[data-slot="ring2"] .doll-slot').click();
  const before = await page.evaluate(() => localStorage.getItem('pobr-v2-build-state'));
  const row = page.locator('.library-row').first();
  await row.hover();
  await expect(page.locator('.shell-changebar .cb')).toBeVisible({ timeout: 30_000 });
  await expect(page.locator('.shell-changebar .cb li').first()).toBeVisible();
  await page.mouse.move(2, 2);
  await expect(page.locator('.shell-changebar')).toHaveCount(0);
  expect(await page.evaluate(() => localStorage.getItem('pobr-v2-build-state'))).toBe(before);
});

test('hovering a replacement position on Upgrades previews its effect from the result it already has', async ({ page }) => {
  await importBuild(page);
  // A replacement needs something equipped to replace: put a ring on first.
  await page.goto('/items');
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 90_000 });
  await page.locator('.doll-cell[data-slot="ring1"] .doll-slot').click();
  await page.getByRole('button', { name: 'Save & recalculate', exact: true }).click();
  await expect(page.locator('.shell-save-state')).toHaveText('Saved locally');
  await page.goto('/trade');
  await page.getByRole('textbox', { name: 'Complete item text' }).fill('Rarity: RARE\nNew Item\nSapphire Ring\n+50 to maximum Life');
  // Typing is not pasting, so ask for the comparison explicitly.
  await page.getByRole('button', { name: 'Calculate replacement', exact: true }).click();
  const position = page.locator('.replacement-position').first();
  await expect(position).toBeVisible({ timeout: 90_000 });
  await position.hover();
  await expect(page.locator('.shell-changebar .cb')).toBeVisible({ timeout: 10_000 });
  await page.mouse.move(2, 2);
  await expect(page.locator('.shell-changebar')).toHaveCount(0);
});

test('hovering a gem upgrade on Upgrades previews its effect from the stats it already carries', async ({ page }) => {
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  // Bound the market pool the same way trade-upgrades.spec.ts does; the calculations stay real.
  await installTradeCatalogFixture(page, catalog => { catalog.gems = catalog.gems?.filter(gem => ['Fireball', 'Controlled Destruction'].includes(gem.name)); });
  await importBuild(page);
  await page.goto('/trade');
  await page.locator('.trade-position').filter({ hasText: 'Skill and support gems' }).click();
  await page.getByRole('button', { name: 'Calculate affix scores', exact: true }).click();
  const card = page.locator('.trade-gem-card').first();
  await expect(card).toBeVisible({ timeout: 90_000 });
  await card.hover();
  await expect(page.locator('.shell-changebar .cb')).toBeVisible({ timeout: 10_000 });
  await page.mouse.move(2, 2);
  await expect(page.locator('.shell-changebar')).toHaveCount(0);
});
