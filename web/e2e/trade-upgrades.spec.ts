import { expect, test } from '@playwright/test';
import { deflateSync } from 'node:zlib';

const searchQuery = (href: string) => JSON.parse(new URL(href).searchParams.get('q')!);

test('essence passive options use real-WASM gains and Puppet Master can be required without a damage weight', async ({ page }) => {
  const code = (nodes: string) => deflateSync(`<PathOfBuilding2>
    <Build level="85" className="Witch"/>
    <Tree activeSpec="1"><Spec nodes="${nodes}" treeVersion="0_5"/></Tree>
    <Skills/>
    <Items activeItemSet="1">
      <Item id="1">Rarity: RARE
Reference Robe
Vile Robe
Implicits: 0
+97 to maximum Life
{crafted}Allocates Goring</Item>
      <Item id="2">Rarity: RARE
Reference Sceptre
Rattling Sceptre
Implicits: 0</Item>
      <ItemSet id="1"><Slot name="Body Armour" itemId="1"/><Slot name="Weapon 1" itemId="2"/></ItemSet>
    </Items>
  </PathOfBuilding2>`).toString('base64url');
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  for (const nodes of ['', '47316']) {
    await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code(nodes));
    await page.locator('.import-submit').click();
    await expect(page.locator('.paper-doll')).toBeVisible();
    await page.getByRole('button', { name: 'Upgrades', exact: true }).click();
    await page.getByRole('button', { name: 'Max Life', exact: true }).click();
    await page.locator('.trade-position').filter({ hasText: 'Body Armour' }).click();
    await page.getByRole('button', { name: 'Calculate affix scores', exact: true }).click();
    const reference = page.locator('.upgrade-score-reference');
    await expect(reference).toContainText('Comparable stat score', { timeout: 60_000 });
    const query = searchQuery((await page.locator('.trade-market-link').first().getAttribute('href'))!);
    const sum = query.query.stats[0];
    const value = (id: string) => sum.filters.find((filter: { id: string }) => filter.id === id)?.value.weight;
    const goring = value('explicit.stat_2954116742|47316');
    const life = value('explicit.stat_3299347043');
    if (nodes) expect(goring).toBeUndefined();
    else expect(goring).toBeGreaterThan(0);
    expect(sum.value.min).toBeCloseTo(97 * life + (goring ?? 0), 3);
    if (!nodes) await page.getByRole('button', { name: 'Build', exact: true }).click();
  }
  await page.locator('.trade-position').filter({ hasText: /^Main Hand/ }).click();
  await page.getByRole('button', { name: 'Calculate affix scores', exact: true }).click();
  const puppet = page.locator('.trade-situational label').filter({ hasText: 'Puppet Master stack' });
  await expect(puppet).toContainText('not modeled', { timeout: 60_000 });
  await puppet.getByRole('checkbox').check();
  const query = searchQuery((await page.locator('.trade-market-link').first().getAttribute('href'))!);
  const required = query.query.stats.find((group: { type: string }) => group.type === 'and');
  expect(required.filters).toContainEqual({ id: 'explicit.stat_2840930496' });
  const weighted = query.query.stats.find((group: { type: string }) => group.type === 'weight');
  expect(weighted?.filters.some((filter: { id: string }) => filter.id === 'explicit.stat_2840930496') ?? false).toBe(false);
});

test('alloy hybrid effects enter real-WASM weights and the crafted item market minimum', async ({ page }) => {
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: 85, class_name: 'Sorceress' },
    equipments: [{ inventoryId: 'Weapon', baseType: 'Attuned Wand', name: 'Crafted Reference', frameType: 2,
      craftedMods: ['28% increased Cast Speed', 'Gain 8% of Elemental Damage as Extra Cold Damage'] }],
    talent_tree: { hashes: [54447, 4739, 22419, 18407], quest_stats: [] }, jewel_data: '[]',
    skills: [{ baseType: 'Fireball', support: false, properties: [{ type: 5, values: [['16', 0]] }] }],
  } }));
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticAlloyFixture');
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible();
  await page.getByRole('button', { name: 'Upgrades', exact: true }).click();
  await page.getByRole('button', { name: 'Max total DPS', exact: true }).click();
  await page.locator('.trade-position').filter({ hasText: /^Main Hand/ }).click();
  await page.getByRole('button', { name: 'Calculate affix scores', exact: true }).click();
  const reference = page.locator('.upgrade-score-reference');
  await expect(reference).toContainText('Comparable stat score');
  const query = searchQuery((await page.locator('.trade-market-link').first().getAttribute('href'))!);
  const sum = query.query.stats[0];
  const value = (id: string) => sum.filters.find((filter: { id: string }) => filter.id === id)?.value.weight;
  const cold = value('explicit.stat_1158842087');
  const speed = value('explicit.stat_2891184298');
  expect(cold).toBeGreaterThan(0);
  expect(speed).toBeGreaterThan(0);
  expect(new Set(sum.filters.map((filter: { id: string }) => filter.id)).size).toBe(sum.filters.length);
  expect(sum.value.min).toBeCloseTo(8 * cold + 28 * speed, 3);
  await reference.getByText(/Current item score breakdown/).click();
  await expect(reference.locator('li')).toHaveCount(2);
});

test('imported PoB armour and belt use their full explicit Sum as the market minimum', async ({ page }) => {
  const code = deflateSync(`<PathOfBuilding2>
    <Build level="85" className="Witch"/>
    <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
    <Skills/>
    <Items activeItemSet="1">
      <Item id="1">
        Rarity: RARE
        Reference Robe
        Vile Robe
        Sockets: S
        Rune: Iron Rune
        Implicits: 1
        {enchant}{rune}20% increased Armour, Evasion and Energy Shield
        +97 to maximum Life
        {crafted}8% increased maximum Life
        {desecrated}12% increased Spirit Reservation Efficiency of Skills
      </Item>
      <Item id="2">
        Rarity: RARE
        Reference Belt
        Double Belt
        Charm Slots: 3
        Implicits: 1
        Has 3 Charm Slots
        +75 to maximum Life
      </Item>
      <ItemSet id="1"><Slot name="Body Armour" itemId="1"/><Slot name="Belt" itemId="2"/></ItemSet>
    </Items>
  </PathOfBuilding2>`).toString('base64url');
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible();
  await page.getByRole('button', { name: 'Upgrades', exact: true }).click();
  await page.getByRole('button', { name: 'Max Life', exact: true }).click();
  for (const [slot, life] of [['Body Armour', 97], ['Belt', 75]] as const) {
    await page.locator('.trade-position').filter({ hasText: slot }).click();
    await page.getByRole('button', { name: 'Calculate affix scores', exact: true }).click();
    const reference = page.locator('.upgrade-score-reference');
    await expect(reference).toContainText('Comparable stat score', { timeout: 60_000 });
    const query = searchQuery((await page.locator('.trade-market-link').first().getAttribute('href'))!);
    const sum = query.query.stats[0];
    const lifeWeight = sum.filters.find((filter: { id: string }) => filter.id === 'explicit.stat_3299347043').value.weight;
    expect(lifeWeight).toBeGreaterThan(0);
    const percent = slot === 'Body Armour' ? sum.filters.find((filter: { id: string }) => filter.id === 'explicit.stat_983749596')?.value.weight : 0;
    if (slot === 'Body Armour') expect(percent).toBeGreaterThan(0);
    expect(sum.value.min).toBeCloseTo(life * lifeWeight + 8 * percent, 3);
    expect(Number((await reference.locator('div > strong').first().innerText()).replaceAll(',', ''))).toBeCloseTo(life * lifeWeight + 8 * percent, 3);
  }
});

test('local affix scores work for empty slots and budget edits only update market links', async ({ page }) => {
  await page.route('**/api/trade/leagues?realm=intl', route => route.fulfill({ json: { leagues: ['Future League', 'Standard'] } }));
  let marketCalls = 0;
  await page.route('**/api/trade/search', route => { marketCalls++; return route.fulfill({ status: 401, json: { error: 'Login required' } }); });
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Character' })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('button', { name: 'Upgrades', exact: true }).click();
  await expect(page.getByRole('button', { name: 'League', exact: true })).toContainText('Future League');
  await page.getByRole('button', { name: 'Max Life', exact: true }).click();
  await page.locator('.trade-position').filter({ hasText: /^Amulet/ }).click();
  await page.getByRole('button', { name: 'Calculate affix scores', exact: true }).click();
  const table = page.locator('.trade-score-table');
  await expect(table).toBeVisible({ timeout: 90_000 });
  await expect(table).toContainText('maximum Life');
  const originalScores = await table.innerText();
  const link = page.locator('.trade-market-link').first();
  const query = searchQuery((await link.getAttribute('href'))!);
  expect(query.query.filters.type_filters.filters.category.option).toBe('accessory.amulet');
  expect(query.query.filters.trade_filters.filters.price).toEqual({ max: 100 });
  expect(query.query.status.option).toBe('online');
  expect(query.query.stats[0].filters.length).toBeGreaterThan(0);
  expect(query.query.type).toBeUndefined();
  expect(query.query.filters.type_filters.filters.rarity.option).toBe('nonunique');
  await page.getByRole('spinbutton', { name: 'Max price' }).fill('50');
  expect(searchQuery((await link.getAttribute('href'))!).query.filters.trade_filters.filters.price.max).toBe(50);
  expect(await table.innerText()).toBe(originalScores);
  const currency = page.getByRole('button', { name: 'Budget currency', exact: true });
  for (const [label, id] of [
    ['Exalted or Divine Orbs', 'exalted_divine'], ['Exalted Orb Equivalent', 'equiv'],
    ['Divine', 'divine'], ['Chaos', 'chaos'], ['Exalted', 'exalted'],
    ['Regal', 'regal'], ['Alchemy', 'alch'], ['Vaal', 'vaal'], ['Annulment', 'annul'],
    ['Augmentation', 'aug'], ['Transmutation', 'transmute'], ['Mirror of Kalandra', 'mirror'],
  ]) {
    await currency.click();
    await page.getByRole('option', { name: label, exact: true }).click();
    expect(searchQuery((await link.getAttribute('href'))!).query.filters.trade_filters.filters.price).toEqual(
      id === 'equiv' ? { max: 50 } : { max: 50, option: id },
    );
    expect(await table.innerText()).toBe(originalScores);
    expect(await page.evaluate(() => localStorage.getItem('pobr-trade-currency'))).toBe(id);
  }
  await page.getByRole('checkbox', { name: 'Include unique items' }).check();
  expect(searchQuery((await link.getAttribute('href'))!).query.filters.type_filters.filters.rarity).toBeUndefined();
  expect(await table.innerText()).toBe(originalScores);
  await page.getByRole('button', { name: 'Max total DPS', exact: true }).click();
  await expect(table).toHaveCount(0);
  await expect(link).toHaveCount(0);
  expect(marketCalls).toBe(0);
  await page.reload();
  await expect(currency).toContainText('Mirror of Kalandra', { timeout: 90_000 });
  await expect(page.getByRole('spinbutton', { name: 'Max price' })).toHaveValue('50');
});

for (const [savedCurrency, label] of [
  ['equiv', 'Exalted Orb Equivalent'], ['exalted_divine', 'Exalted or Divine Orbs'],
  ['exalted', 'Exalted'], ['unknown-currency', 'Exalted Orb Equivalent'],
]) test(`saved ${savedCurrency} currency restores as ${label}`, async ({ page }) => {
  await page.addInitScript(value => localStorage.setItem('pobr-trade-currency', value), savedCurrency);
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Character' })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('button', { name: 'Upgrades', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Budget currency', exact: true }).locator('.app-select-value')).toHaveText(label);
});

test('WeGame quiver analysis opens the CN instant-buy market without JSON or base selection', async ({ page, context }) => {
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Test League'] } }));
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: 71, class_name: 'Deadeye' },
    equipments: [
      { inventoryId: 'Weapon', baseType: 'Crude Bow', frameType: 0, explicitMods: [{ description: 'Adds 10 to 20 Physical Damage' }] },
      { inventoryId: 'Offhand', baseType: 'Primed Quiver', name: 'Current Quiver', frameType: 2, implicitMods: ['8% increased Attack Speed'] },
    ],
    talent_tree: { hashes: [], quest_stats: [] }, jewel_data: '[]',
    skills: [{ baseType: 'Ice Shot', support: false, properties: [{ type: 5, values: [['12', 0]] }] }],
  } }));
  let marketCalls = 0;
  await page.route('**/api/trade/search', route => { marketCalls++; return route.abort(); });
  await context.route('https://poe.game.qq.com/**', route => route.fulfill({ contentType: 'text/html', body: '<h1>Official market fixture</h1>' }));
  await page.goto('/');
  await expect(page.getByRole('heading', { name: /Import Build/i })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticShareKey_123456');
  await page.locator('.import-submit').click();
  await expect(page.getByRole('heading', { name: 'Items', exact: true })).toBeVisible({ timeout: 60_000 });
  await page.getByRole('button', { name: 'Upgrades', exact: true }).click();
  await page.getByRole('button', { name: 'Server', exact: true }).click();
  await page.getByRole('option', { name: 'CN (Tencent)', exact: true }).click();
  await page.locator('.trade-position').filter({ hasText: /^Off Hand/ }).click();
  await expect(page.locator('.trade-scope')).toContainText('Quiver');
  await expect(page.getByRole('button', { name: 'Skill used for scoring' })).toContainText('Ice Shot');
  await page.getByRole('button', { name: 'Calculate affix scores', exact: true }).click();
  await expect(page.locator('.trade-score-table')).toBeVisible({ timeout: 90_000 });
  const utility = page.locator('.trade-situational label').filter({ hasText: /Surpassing/ }).first();
  await expect(utility).toBeVisible();
  await utility.getByRole('checkbox').check();
  const link = page.locator('.trade-market-link').first();
  const query = searchQuery((await link.getAttribute('href'))!);
  expect(query.query.stats.at(-1).filters).toContainEqual({ id: 'explicit.stat_2463230181' });
  expect(query.query.filters.type_filters.filters.category.option).toBe('armour.quiver');
  expect(query.query.filters.req_filters.filters.lvl.max).toBe(71);
  expect(query.query.status.option).toBe('any');
  expect(query.query.type).toBeUndefined();
  await page.locator('.trade-broad input').check();
  expect(searchQuery((await link.getAttribute('href'))!).query.stats[0].value).toBeUndefined();
  await expect(page.locator('.trade-page input[type="file"]')).toHaveCount(0);
  await expect(page.locator('.trade-page a[href^="javascript:"]')).toHaveCount(0);
  const opened = page.waitForEvent('popup');
  await link.click();
  const market = await opened;
  await expect(market.getByRole('heading', { name: 'Official market fixture' })).toBeVisible();
  expect(new URL(market.url()).hostname).toBe('poe.game.qq.com');
  expect(marketCalls).toBe(0);
});

for (const [characterLevel, gemLevel] of [[71, 16], [90, 21]]) test(`gem plans at character level ${characterLevel} use wearable levels and localized links`, async ({ page }) => {
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.route('**/overlay/trade_catalog.json', async route => {
    const catalog = await (await route.fetch()).json();
    // Bound the integration pool; retain real gem data and real WASM calculations.
    catalog.gems = catalog.gems.filter((gem: { name: string }) => ['Fireball', 'Controlled Destruction'].includes(gem.name));
    await route.fulfill({ json: catalog });
  });
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: characterLevel, class_name: 'Witch' }, equipments: [],
    talent_tree: { hashes: [], quest_stats: [] }, jewel_data: '[]',
    skills: [{ baseType: 'Fireball', support: false, properties: [{ type: 5, values: [['12', 0]] }] }],
  } }));
  await page.goto('/');
  await expect(page.getByRole('heading', { name: /Import Build/i })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticShareKey_123456');
  await page.locator('.import-submit').click();
  await expect(page.getByRole('heading', { name: 'Items', exact: true })).toBeVisible({ timeout: 60_000 });
  await page.getByRole('button', { name: 'Upgrades', exact: true }).click();
  await page.locator('.trade-position').filter({ hasText: 'Skill and support gems' }).click();
  await page.getByRole('button', { name: 'Calculate affix scores', exact: true }).click();
  const card = page.locator('.trade-gem-card').first();
  await expect(card).toContainText('Fireball', { timeout: 90_000 });
  await expect(card.locator('.trade-gain')).toContainText('+');
  const original = searchQuery((await card.getByRole('link').getAttribute('href'))!);
  expect(original.query.type).toBe('Fireball');
  expect(original.query.filters.misc_filters.filters.gem_level.min).toBe(gemLevel);
  expect(original.query.filters.type_filters.filters.quality.min).toBe(20);
  // Market language follows the realm even when the calculator UI is English.
  await page.getByRole('button', { name: 'Server', exact: true }).click();
  await page.getByRole('option', { name: 'CN (Tencent)', exact: true }).click();
  const cn = searchQuery((await card.getByRole('link').getAttribute('href'))!);
  expect(cn.query.type).toBe('火球');
  expect(cn.query.status.option).toBe('any');
  const state = await page.evaluate(() => {
    const saved = Object.values(localStorage).find(value => value.includes('"socketGroups"'))!;
    return JSON.parse(saved).state;
  });
  expect(state.socketGroups[0].gems).toEqual([{ skill_id: 'FireballPlayer', level: 12, quality: 0 }]);
});

test('default damage skill, weapon binding and whole-build priority use the same build context', async ({ page }) => {
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.route('**/overlay/trade_catalog.json', async route => {
    const catalog = await (await route.fetch()).json();
    // Exercise real calculations with a small legal pool so the integration test stays fast.
    catalog.mods = catalog.mods.filter((mod: { lines: string[] }) => mod.lines.some(line => /to maximum Life|increased Spell Damage|increased Cast Speed|to Fire Resistance/.test(line))).slice(0, 12);
    catalog.gems = catalog.gems.filter((gem: { name: string }) => gem.name === 'Fireball');
    await route.fulfill({ json: catalog });
  });
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: 71, class_name: 'Witch' },
    equipments: [
      { inventoryId: 'Weapon', baseType: 'Crude Bow', frameType: 0 },
      { inventoryId: 'Offhand', baseType: 'Primed Quiver', frameType: 0 },
      { inventoryId: 'Weapon2', baseType: 'Ashen Staff', frameType: 2, name: 'Spell Staff', explicitMods: ['100% increased Spell Damage'] },
      { inventoryId: 'Ring', x: 0, baseType: 'Sapphire Ring', frameType: 0 },
    ], talent_tree: { hashes: [], quest_stats: [] }, jewel_data: '[]',
    skills: [{ baseType: 'Raise Shield', support: false },
      { baseType: 'Fireball', support: false, properties: [{ type: 5, values: [['12', 0]] }] },
      { baseType: 'Ice Shot', support: false, properties: [{ type: 5, values: [['1', 0]] }] }],
  } }));
  await page.goto('/');
  await expect(page.getByRole('heading', { name: /Import Build/i })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticShareKey_123456');
  await page.locator('.import-submit').click();
  await expect(page.getByRole('heading', { name: 'Items', exact: true })).toBeVisible({ timeout: 60_000 });
  await expect(page.locator('.main-skill-select')).toHaveValue('1');
  await page.getByRole('button', { name: 'Skills', exact: true }).click();
  await page.locator('.skill-group-title').filter({ hasText: 'Fireball' }).click();
  await page.getByRole('button', { name: 'Skill weapon set', exact: true }).click();
  await page.getByRole('option', { name: 'Set 2', exact: true }).click();
  await expect(page.getByRole('group', { name: 'Active weapon set' }).getByRole('button', { name: 'Set 2', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await page.getByRole('button', { name: 'Items', exact: true }).click();
  await expect(page.locator('.paper-doll')).toContainText('Spell Staff');
  await expect(page.locator('.paper-doll')).not.toContainText('Primed Quiver');
  await page.getByRole('button', { name: 'Upgrades', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Skill used for scoring' })).toContainText('Fireball');
  await expect(page.getByRole('checkbox', { name: 'Keep at least current EHP' })).toBeChecked();
  await page.getByRole('button', { name: 'Analyze all positions', exact: true }).click();
  await expect(page.locator('.trade-priorities')).toBeVisible({ timeout: 90_000 });
  await expect(page.getByRole('button', { name: 'Analyze all positions', exact: true })).toBeEnabled({ timeout: 90_000 });
  const ranking = await page.locator('.trade-priorities').innerText();
  expect(ranking).toContain('DPS');
  expect(ranking).toContain('EHP');
  await page.getByRole('checkbox', { name: 'Prioritize capped elemental resistances' }).check();
  await expect(page.locator('.trade-priority-position')).toHaveCount(0);
  await page.getByRole('button', { name: 'Items', exact: true }).click();
  await page.getByRole('group', { name: 'Active weapon set' }).getByRole('button', { name: 'Set 1', exact: true }).click();
  await expect(page.locator('.paper-doll')).toContainText('Primed Quiver');
  await page.reload();
  await expect(page.locator('.paper-doll')).toContainText('Primed Quiver', { timeout: 90_000 });
  await page.getByRole('group', { name: 'Active weapon set' }).getByRole('button', { name: 'Set 2', exact: true }).click();
  await expect(page.locator('.paper-doll')).toContainText('Spell Staff');
});
