import { expect, test } from '@playwright/test';

test('trade searches new affix combinations with a category and live league', async ({ page }) => {
  await page.route('**/api/trade/leagues?realm=intl', route => route.fulfill({
    json: { leagues: ['Future League', 'Standard'] },
  }));
  await page.route('**/api/trade/search', async route => {
    const query = route.request().postDataJSON();
    expect(query.category).toBe('accessory.amulet');
    expect(query.league).toBe('Future League');
    expect(query.price).toEqual({ max: 100, currency: 'exalted' });
    expect(query.weighted.length).toBeGreaterThan(0);
    await route.fulfill({ json: { url: 'https://www.pathofexile.com/trade2/search/poe2/Future%20League/synthetic',
      total: 20, sampled: 2, listings: [
        { id: 'single', price: { amount: 80, currency: 'exalted' }, item: { name: 'Single Affix', baseType: 'Amber Amulet', frameType: 2, explicitMods: [{ description: '+30 to maximum Life' }] } },
        { id: 'combo', price: { amount: 10, currency: 'exalted' }, item: { name: 'Better Combination', baseType: 'Amber Amulet', frameType: 2, explicitMods: [{ description: '+70 to maximum Life' }, { description: '+20 to Strength' }] } },
      ] } });
  });
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Character' })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('button', { name: 'Trade', exact: true }).click();
  await expect(page.getByRole('button', { name: 'League', exact: true })).toContainText('Future League');
  await page.getByRole('button', { name: 'Objective', exact: true }).click();
  await page.getByRole('option', { name: 'Max Life', exact: true }).click();
  const offhand = page.locator('.trade-slot-row').filter({ has: page.locator('.trade-slot-name', { hasText: /^Off Hand$/ }) });
  await offhand.getByLabel('Off Hand Item type').selectOption('armour.quiver');
  await offhand.getByLabel('Off Hand Base').selectOption('Broadhead Quiver');
  const row = page.locator('.trade-slot-row').filter({ has: page.locator('.trade-slot-name', { hasText: /^Amulet$/ }) });
  // Empty slots must work: the search pool comes from the category, not current mods.
  await row.getByRole('button', { name: 'Find better', exact: true }).click();
  await expect(row.locator('.trade-market-card')).toHaveCount(2, { timeout: 90_000 });
  await expect(row.locator('.trade-market-card').first()).toContainText('Better Combination');
  await expect(row.locator('.trade-market-card').first()).toContainText('10 exalted');
  const href = await row.locator('.trade-slot-main > a').first().getAttribute('href');
  const query = JSON.parse(new URL(href!).searchParams.get('q')!);
  expect(query.query.filters.type_filters.filters.category.option).toBe('accessory.amulet');
  // Changing market inputs must invalidate prices and calculated results.
  await page.getByRole('spinbutton', { name: 'Max price' }).fill('50');
  await expect(row.locator('.trade-market-card')).toHaveCount(0);
  await expect(row.getByRole('link')).toHaveCount(0);
});

test('gem purchases use actual listing level and replace the existing gem', async ({ page }) => {
  await page.route('**/api/trade/leagues?realm=intl', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.route('**/overlay/trade_catalog.json', async route => {
    const response = await route.fetch();
    const catalog = await response.json();
    // Keep a small real-data pool so this integration check does not benchmark hundreds of supports.
    catalog.gems = catalog.gems.filter((gem: { name: string }) => ['Fireball', 'Controlled Destruction'].includes(gem.name));
    await route.fulfill({ json: catalog });
  });
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: 90, class_name: 'Witch' }, equipments: [],
    talent_tree: { hashes: [], quest_stats: [] }, jewel_data: '[]',
    skills: [{ baseType: 'Fireball', support: false, properties: [{ type: 5, values: [['12', 0]] }] }],
  } }));
  await page.route('**/api/trade/search', route => {
    const query = route.request().postDataJSON();
    expect(query.category).toBe('gem');
    return route.fulfill({ json: { url: 'https://www.pathofexile.com/trade2/search/poe2/Standard/synthetic',
      total: 1, sampled: 1, listings: [{ id: query.gem.name, price: { amount: 10, currency: 'exalted' },
        item: { baseType: query.gem.name, frameType: 4, properties: [
          { type: 5, values: [[query.gem.name === 'Fireball' ? '21' : '1', 0]] }, { type: 6, values: [['+20%', 1]] },
        ] } }],
    } });
  });
  await page.goto('/');
  await expect(page.getByRole('heading', { name: /Import Build/i })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticShareKey_123456');
  await page.locator('.import-submit').click();
  await expect(page.getByRole('heading', { name: 'Items' })).toBeVisible({ timeout: 60_000 });
  await page.getByRole('button', { name: 'Trade', exact: true }).click();
  const panel = page.locator('.trade-gems');
  await expect(panel.getByRole('button', { name: 'Find better' })).toBeEnabled();
  await panel.getByRole('button', { name: 'Find better' }).click();
  await expect(panel.locator('.trade-market-card').first()).toContainText('Fireball', { timeout: 90_000 });
  await panel.locator('.trade-market-card').first().getByRole('button', { name: 'Try on' }).click();
  const state = await page.evaluate(() => {
    const saved = Object.values(localStorage).find(value => value.includes('"socketGroups"'))!;
    return JSON.parse(saved).state;
  });
  expect(state.socketGroups[0].gems).toEqual([{ skill_id: 'FireballPlayer', level: 21, quality: 20 }]);
});

test('signed-in CN bookmark exports real item shapes and imports them for recalculation', async ({ page, context }, testInfo) => {
  const { readFile } = await import('node:fs/promises');
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Test League'] } }));
  await page.route('**/api/trade/search', route => route.fulfill({ status: 401, json: { error: 'Trade access requires verification.' } }));
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Character' })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('button', { name: 'Trade', exact: true }).click();
  await page.getByRole('button', { name: 'Server', exact: true }).click();
  await page.getByRole('option', { name: 'CN (Tencent)', exact: true }).click();
  await page.getByRole('button', { name: 'Objective', exact: true }).click();
  await page.getByRole('option', { name: 'Max Life', exact: true }).click();
  const row = page.locator('.trade-slot-row').filter({ has: page.locator('.trade-slot-name', { hasText: /^Amulet$/ }) });
  await row.getByRole('button', { name: 'Find better', exact: true }).click();
  await expect(row.locator('.opt-error')).toContainText('verification', { timeout: 90_000 });
  const bookmark = await row.getByRole('link', { name: 'Export this search to PoBR' }).getAttribute('href');
  expect(bookmark).toMatch(/^javascript:void/);

  // Test the production-minified bookmark inside a same-origin official tab.
  // Only HTTP is synthetic; credentials, download and WASM recalculation are real browser behavior.
  const official = await context.newPage();
  await context.addCookies([{ name: 'POESESSID', value: 'synthetic-login-secret', url: 'https://poe.game.qq.com' }]);
  const id = 'a'.repeat(64);
  let calls = 0;
  await official.route('https://poe.game.qq.com/**', async route => {
    const request = route.request();
    if (!request.url().includes('/api/')) return route.fulfill({ contentType: 'text/html', body: '<html><body>Official trade fixture</body></html>' });
    calls++;
    expect((await request.allHeaders()).cookie).toContain('POESESSID=synthetic-login-secret');
    if (request.method() === 'POST') {
      expect(request.postDataJSON().query.filters.type_filters.filters.category.option).toBe('accessory.amulet');
      return route.fulfill({ json: { id: 'synthetic', total: 1, result: [id] } });
    }
    return route.fulfill({ json: { result: [{ id, listing: { account: { name: 'SyntheticSeller' }, whisper: 'excluded', price: { amount: 10, currency: 'exalted' } },
      item: { name: 'Imported Upgrade', baseType: 'Amber Amulet', frameType: 2, explicitMods: [{ description: '+70 to maximum Life' }] },
    }] } });
  });
  official.on('dialog', dialog => void dialog.dismiss());
  await official.goto('https://poe.game.qq.com/trade2/search/poe2/Test%20League');
  const downloaded = official.waitForEvent('download');
  await official.evaluate(source => { (0, eval)(source!.slice('javascript:'.length)); }, bookmark);
  const file = testInfo.outputPath('trade-candidates.json');
  await (await downloaded).saveAs(file);
  const content = await readFile(file, 'utf8');
  expect(content).not.toContain('synthetic-login-secret');
  expect(content).not.toContain('excluded');
  expect(calls).toBe(2);
  await row.getByLabel('Amulet Import trade candidates').setInputFiles(file);
  await expect(row.locator('.trade-market-card').first()).toContainText('Imported Upgrade');
  await expect(row.locator('.trade-market-card').first()).toContainText('10 exalted');
  await expect(row.locator(':scope > .opt-error')).toHaveCount(0);
});
