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
  await expect(page.locator('.shell-save-state')).toHaveText('Saved in this browser');
}

test('a number in the results rail opens the calculation behind it, and it can be closed', async ({ page }) => {
  await importBuild(page);
  // An exact label: `hasText` is a substring match and would pick "Enemy life dealt per hit".
  await page.locator('.rail-row.is-link').filter({ has: page.locator('.rail-label', { hasText: /^Life$/ }) }).first().click();
  await expect(page.locator('.insp h2')).toHaveText('Life');
  await expect(page.locator('.insp .formula-final, .insp .breakdown-summary').first()).toBeVisible({ timeout: 30_000 });
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(page.locator('.insp')).toHaveCount(0);
});

for (const width of [390, 1440]) {
  test(`inspector formula and modifier columns fit inside the popover at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await importBuild(page);
    const results = width === 390
      ? page.getByRole('dialog', { name: 'Character stats', exact: true })
      : page.locator('.shell-rail');
    if (width === 390) await page.getByRole('button', { name: 'Character stats', exact: true }).click();
    await expect(results).toBeVisible();
    await results.locator('.rail-row.is-link').filter({ has: page.locator('.rail-label', { hasText: /^Life$/ }) }).locator('.rail-value').click();
    const inspector = page.locator('.insp');
    await expect(inspector.locator('h2')).toHaveText('Life');
    const formula = inspector.locator('.breakdown-formula').first();
    await expect(formula).toBeVisible({ timeout: 30_000 });
    // Stress the real formula styling with long labels and numbers in both numeric columns.
    await formula.evaluate(table => {
      table.querySelectorAll('th').forEach(cell => { cell.textContent = 'Long calculation source '.repeat(20); });
      table.querySelectorAll('.formula-operand, .formula-result').forEach(cell => { cell.textContent = '-2,290,002.123456789'; });
    });
    const measurements = await formula.evaluate(table => {
      const panel = table.closest('.insp')!.getBoundingClientRect();
      const bounds = table.getBoundingClientRect();
      return Array.from(table.querySelectorAll('.formula-operand, .formula-result'), cell => {
        const box = cell.getBoundingClientRect();
        const range = document.createRange();
        range.selectNodeContents(cell);
        const text = range.getBoundingClientRect();
        return { whiteSpace: getComputedStyle(cell).whiteSpace, width: box.width, tableWidth: bounds.width,
          left: box.left, right: box.right, textLeft: text.left, textRight: text.right,
          panelLeft: panel.left, panelRight: panel.right, viewportWidth: innerWidth };
      });
    });
    expect(measurements.length).toBeGreaterThan(0);
    for (const cell of measurements) {
      expect(cell.whiteSpace).toBe('normal');
      expect(cell.width).toBeGreaterThanOrEqual(cell.tableWidth * .24);
      expect(cell.left).toBeGreaterThanOrEqual(cell.panelLeft - 1);
      expect(cell.right).toBeLessThanOrEqual(Math.min(cell.panelRight, cell.viewportWidth) + 1);
      expect(cell.textLeft).toBeGreaterThanOrEqual(cell.left - 1);
      expect(cell.textRight).toBeLessThanOrEqual(cell.right + 1);
    }
    const modifiers = inspector.locator('.modifier-table').first();
    await expect(modifiers).toBeVisible();
    const columns = await modifiers.evaluate(table => {
      table.querySelectorAll('.mod-value').forEach(cell => { cell.textContent = '-2,290,002'; });
      table.querySelectorAll('.mod-text').forEach(cell => { cell.textContent = '角色等级与属性提供的基础值'; });
      table.querySelectorAll('.mod-origin').forEach(cell => { cell.textContent = 'LongCharacterBaseSourceWithoutSpaces'; });
      const panel = table.closest('.insp')!.getBoundingClientRect();
      return Array.from(table.querySelectorAll('tbody tr'), row => {
        const value = row.querySelector('.mod-value')!;
        const valueBox = value.getBoundingClientRect();
        const text = row.querySelector('.mod-text')!.getBoundingClientRect();
        const source = row.querySelector('.mod-origin')!;
        const sourceBox = source.getBoundingClientRect();
        return { valueWhiteSpace: getComputedStyle(value).whiteSpace, valueWidth: valueBox.width,
          valueRight: valueBox.right, textWidth: text.width, sourceWidth: sourceBox.width,
          sourceWhiteSpace: getComputedStyle(source).whiteSpace, sourceRight: sourceBox.right,
          tableWidth: table.getBoundingClientRect().width, panelRight: panel.right, viewportWidth: innerWidth };
      });
    });
    expect(columns.length).toBeGreaterThan(0);
    for (const column of columns) {
      expect(column.valueWhiteSpace).toBe('nowrap');
      expect(column.textWidth).toBeGreaterThan(column.valueWidth);
      expect(column.sourceWhiteSpace).toBe('normal');
      expect(column.sourceWidth).toBeLessThanOrEqual(column.tableWidth * .26 + 1);
      expect(column.valueRight).toBeLessThanOrEqual(Math.min(column.panelRight, column.viewportWidth) + 1);
      expect(column.sourceRight).toBeLessThanOrEqual(Math.min(column.panelRight, column.viewportWidth) + 1);
    }
  });
}

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
  await page.getByRole('button', { name: 'Apply changes', exact: true }).click();
  await page.getByRole('button', { name: 'Save to library', exact: true }).click();
  await expect(page.locator('.shell-save-state')).toHaveText('Saved in this browser');
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
  await page.getByRole('button', { name: 'Apply changes', exact: true }).click();
  await expect(page.locator('.shell-save-state')).toHaveText('Saved in this browser');
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
