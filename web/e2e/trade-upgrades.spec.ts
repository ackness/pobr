import { expect, test } from '@playwright/test';
import { openItemsAfterImport } from './shell';
import { dpsValue, goTo, inspectedSkill, openLibrary, openMarketSettings } from './upgradesPage';
import { deflateSync } from 'node:zlib';
import { installTradeCatalogFixture } from './tradeCatalogFixture';

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
  await page.goto('/build');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  for (const nodes of ['', '47316']) {
    await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code(nodes));
    await page.locator('.import-submit').click();
    await openItemsAfterImport(page);
    await expect(page.locator('.paper-doll')).toBeVisible();
    await goTo(page, 'Upgrades');
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
    if (!nodes) await openLibrary(page);
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
  await page.goto('/build');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticAlloyFixture');
  await page.locator('.import-submit').click();
  await openItemsAfterImport(page);
  await expect(page.locator('.paper-doll')).toBeVisible();
  await goTo(page, 'Upgrades');
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
  await page.goto('/build');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();
  await openItemsAfterImport(page);
  await expect(page.locator('.paper-doll')).toBeVisible();
  await goTo(page, 'Upgrades');
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
  await page.goto('/build');
  await expect(page.getByRole('heading', { name: 'Character' })).toBeVisible({ timeout: 90_000 });
  await goTo(page, 'Upgrades');
  await openMarketSettings(page);
  await expect(page.getByRole('combobox', { name: 'League', exact: true })).toContainText('Future League');
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
  const currency = page.getByRole('combobox', { name: 'Budget currency', exact: true });
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
    expect(await page.evaluate(() => localStorage.getItem('pobr-v2-trade-currency'))).toBe(id);
  }
  await page.getByRole('checkbox', { name: 'Include unique items' }).check();
  expect(searchQuery((await link.getAttribute('href'))!).query.filters.type_filters.filters.rarity).toBeUndefined();
  expect(await table.innerText()).toBe(originalScores);
  await page.getByRole('button', { name: 'Max total DPS', exact: true }).click();
  await expect(table).toHaveCount(0);
  await expect(link).toHaveCount(0);
  expect(marketCalls).toBe(0);
  await page.reload();
  await openMarketSettings(page);
  await expect(currency).toContainText('Mirror of Kalandra', { timeout: 90_000 });
  await expect(page.getByRole('spinbutton', { name: 'Max price' })).toHaveValue('50');
});

for (const [savedCurrency, label] of [
  ['equiv', 'Exalted Orb Equivalent'], ['exalted_divine', 'Exalted or Divine Orbs'],
  ['exalted', 'Exalted'], ['unknown-currency', 'Exalted Orb Equivalent'],
]) test(`saved ${savedCurrency} currency restores as ${label}`, async ({ page }) => {
  await page.addInitScript(value => localStorage.setItem('pobr-v2-trade-currency', value), savedCurrency);
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.goto('/build');
  await expect(page.getByRole('heading', { name: 'Character' })).toBeVisible({ timeout: 90_000 });
  await goTo(page, 'Upgrades');
  await openMarketSettings(page);
  await expect(page.getByRole('combobox', { name: 'Budget currency', exact: true })).toHaveText(label);
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
  await page.goto('/build');
  await expect(page.getByRole('heading', { name: 'Import a build', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticShareKey_123456');
  await page.locator('.import-submit').click();
  await openItemsAfterImport(page);
  await expect(page.getByRole('heading', { name: 'Items', exact: true })).toBeVisible({ timeout: 60_000 });
  await goTo(page, 'Upgrades');
  await openMarketSettings(page);
  await page.getByRole('combobox', { name: 'Server', exact: true }).click();
  await page.getByRole('option', { name: 'CN (Tencent)', exact: true }).click();
  await page.locator('.trade-position').filter({ hasText: /^Off Hand/ }).click();
  await expect(page.locator('.trade-scope')).toContainText('Quiver');
  await expect(page.getByRole('combobox', { name: 'Skill used for scoring' })).toContainText('Ice Shot');
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
  await installTradeCatalogFixture(page, catalog => {
    // Bound the integration pool; retain real gem data and real WASM calculations.
    catalog.gems = catalog.gems?.filter(gem => ['Fireball', 'Controlled Destruction'].includes(gem.name));
  });
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: characterLevel, class_name: 'Witch' }, equipments: [],
    talent_tree: { hashes: [], quest_stats: [] }, jewel_data: '[]',
    skills: [{ baseType: 'Fireball', support: false, properties: [{ type: 5, values: [['12', 0]] }] }],
  } }));
  await page.goto('/build');
  await expect(page.getByRole('heading', { name: 'Import a build', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticShareKey_123456');
  await page.locator('.import-submit').click();
  await openItemsAfterImport(page);
  await expect(page.getByRole('heading', { name: 'Items', exact: true })).toBeVisible({ timeout: 60_000 });
  await page.getByRole('combobox', { name: 'Inspected skill', exact: true }).click();
  await page.getByRole('option', { name: '1. Fireball', exact: true }).click();
  await goTo(page, 'Upgrades');
  await page.locator('.trade-position').filter({ hasText: 'Skill and support gems' }).click();
  await page.getByRole('button', { name: 'Calculate affix scores', exact: true }).click();
  // Other gems (lineage supports from the full game data) can outrank Fireball; this test is about Fireball's own plan.
  const card = page.locator('.trade-gem-card').filter({ hasText: 'Fireball' }).first();
  await expect(card).toContainText('Fireball', { timeout: 90_000 });
  await expect(card.locator('.trade-gain')).toContainText('+');
  const original = searchQuery((await card.getByRole('link').getAttribute('href'))!);
  expect(original.query.type).toBe('Fireball');
  expect(original.query.filters.misc_filters.filters.gem_level.min).toBe(gemLevel);
  expect(original.query.filters.type_filters.filters.quality.min).toBe(20);
  // Market language follows the realm even when the calculator UI is English.
  await openMarketSettings(page);
  await page.getByRole('combobox', { name: 'Server', exact: true }).click();
  await page.getByRole('option', { name: 'CN (Tencent)', exact: true }).click();
  const cn = searchQuery((await card.getByRole('link').getAttribute('href'))!);
  expect(cn.query.type).toBe('火球');
  expect(cn.query.status.option).toBe('any');
  const state = await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state);
  expect(state.socketGroups[0].gems).toEqual([{ skill_id: 'FireballPlayer', level: 12, quality: 0 }]);
});

test('explicit damage skill selection, weapon binding and whole-build priority share the same context', async ({ page }) => {
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await installTradeCatalogFixture(page, catalog => {
    // Exercise real calculations with a small legal pool so the integration test stays fast.
    catalog.mods = catalog.mods.filter(mod => mod.lines.some(line => /to maximum Life|increased Spell Damage|increased Cast Speed|to Fire Resistance/.test(line))).slice(0, 12);
    catalog.gems = catalog.gems?.filter(gem => gem.name === 'Fireball');
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
  await page.goto('/build');
  await expect(page.getByRole('heading', { name: 'Import a build', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticShareKey_123456');
  await page.locator('.import-submit').click();
  await openItemsAfterImport(page);
  await expect(page.getByRole('heading', { name: 'Items', exact: true })).toBeVisible({ timeout: 60_000 });
  // The import carries no selection; the highest-DPS skill becomes the default.
  await expect(inspectedSkill(page)).not.toHaveText('Not selected');
  await goTo(page, 'Upgrades');
  const target = page.getByRole('combobox', { name: 'Skill used for scoring', exact: true });
  await expect(target).not.toContainText('Not selected');
  await target.click();
  await page.getByRole('option', { name: '2. Fireball', exact: true }).click();
  await expect(inspectedSkill(page)).toHaveText('2. Fireball');
  await goTo(page, 'Skills');
  await page.locator('.skill-group-title').filter({ hasText: 'Fireball' }).click();
  await page.getByRole('combobox', { name: 'Skill weapon set', exact: true }).click();
  await page.getByRole('option', { name: 'Set 2', exact: true }).click();
  await expect(page.getByRole('group', { name: 'Active weapon set' }).getByRole('button', { name: 'Set 2', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await goTo(page, 'Items');
  await expect(page.locator('.paper-doll')).toContainText('Spell Staff');
  await expect(page.locator('.paper-doll')).not.toContainText('Primed Quiver');
  await goTo(page, 'Upgrades');
  await expect(page.getByRole('combobox', { name: 'Skill used for scoring' })).toContainText('Fireball');
  await expect(page.getByRole('checkbox', { name: 'Keep at least current EHP' })).toBeChecked();
  await page.getByRole('button', { name: 'Analyze all positions', exact: true }).click();
  await expect(page.locator('.trade-priorities')).toBeVisible({ timeout: 90_000 });
  await expect(page.locator('.trade-analyze-all')).toBeEnabled({ timeout: 90_000 });
  const ranking = await page.locator('.trade-priorities').innerText();
  expect(ranking).toContain('DPS');
  expect(ranking).toContain('EHP');
  await page.getByRole('checkbox', { name: 'Prioritize capped elemental resistances' }).check();
  await expect(page.locator('.trade-priority-position')).toHaveCount(0);
  await goTo(page, 'Items');
  await page.getByRole('group', { name: 'Active weapon set' }).getByRole('button', { name: 'Set 1', exact: true }).click();
  await expect(page.locator('.paper-doll')).toContainText('Primed Quiver');
  await page.reload();
  await expect(page.locator('.paper-doll')).toContainText('Primed Quiver', { timeout: 90_000 });
  await page.getByRole('group', { name: 'Active weapon set' }).getByRole('button', { name: 'Set 2', exact: true }).click();
  await expect(page.locator('.paper-doll')).toContainText('Spell Staff');
});

test('Time-Lost jewel search scores only allocated nearby passives with real WASM', async ({ page }) => {
  const code = (nodes: string) => deflateSync(`<PathOfBuilding2>
    <Build level="85" className="Witch" mainSocketGroup="1"/>
    <Tree activeSpec="1"><Spec nodes="${nodes}" treeVersion="0_5"><Sockets><Socket nodeId="2491" itemId="1"/></Sockets></Spec></Tree>
    <Skills><Skill enabled="true"><Gem gemId="Metadata/Items/Gems/SkillGemFireball" skillId="FireballPlayer" level="16" quality="0" enabled="true"/></Skill></Skills>
    <Items activeItemSet="1"><Item id="1">Rarity: RARE
Radius Reference
Time-Lost Sapphire
Radius: Small
Implicits: 0
Small Passive Skills in Radius also grant 2% increased Spell Damage</Item><ItemSet id="1"/></Items>
  </PathOfBuilding2>`).toString('base64url');
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.goto('/build');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  for (const nodes of ['2491,9583,47316', '2491']) {
    await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code(nodes));
    await page.locator('.import-submit').click();
    await openItemsAfterImport(page);
    await expect(page.locator('.paper-doll')).toBeVisible();
    await goTo(page, 'Upgrades');
    await page.locator('.trade-position').filter({ hasText: 'Jewel socket 2491' }).click();
    await expect(page.getByRole('combobox', { name: 'Jewel type' })).toContainText('Time-Lost jewel search');
    await page.getByRole('button', { name: 'Max total DPS', exact: true }).click();
    await page.getByRole('button', { name: 'Calculate affix scores', exact: true }).click();
    await expect(page.locator('.upgrade-score-reference')).toBeVisible({ timeout: 60_000 });
    const query = searchQuery((await page.locator('.trade-market-link').first().getAttribute('href'))!);
    const weights = query.query.stats.find((group: { type: string }) => group.type === 'weight')?.filters ?? [];
    const spell = weights.find((stat: { id: string }) => stat.id === 'explicit.stat_1137305356');
    if (nodes.includes('9583')) {
      expect(spell.value.weight).toBeGreaterThan(0);
      expect(weights.some((stat: { id: string }) => stat.id === 'explicit.stat_1022759479')).toBe(true);
      await page.screenshot({ path: test.info().outputPath('time-lost-search.png'), fullPage: true });
    } else expect(weights).toHaveLength(0);
    await openLibrary(page);
  }
});

test('applying a gem replacement preserves the evaluated secondary effect through save and reload', async ({ page }) => {
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await installTradeCatalogFixture(page, catalog => {
    catalog.gems = catalog.gems?.filter(gem => [
      'ElementalWeaknessPlayer', 'ViciousHexSupportPlayer', 'SparkPlayer', 'FireballPlayer',
      'SupportRapidCastingPlayerTwo',
    ].includes(gem.skill_id));
  });
  const code = deflateSync(`<PathOfBuilding2><Build level="80" className="Witch" mainSocketGroup="1"/>
    <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
    <Skills><Skill enabled="true" mainActiveSkill="3" set1="false" set2="true">
      <Gem skillId="ElementalWeaknessPlayer" level="15" quality="20" enabled="true"/>
      <Gem skillId="ViciousHexSupportPlayer" level="1" quality="20" enabled="true"/>
      <Gem skillId="SparkPlayer" level="15" quality="20" enabled="true"/>
      <Gem skillId="FireballPlayer" level="15" quality="20" enabled="true"/>
    </Skill></Skills><Items/><Config/></PathOfBuilding2>`).toString('base64url');
  await page.goto('/build');
  await expect(page.getByRole('textbox', { name: 'Build code' })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill(code);
  await page.locator('.import-submit').click();
  await expect(inspectedSkill(page)).toHaveText('1. Spark');
  await goTo(page, 'Upgrades');
  await expect(page.getByRole('combobox', { name: 'Skill used for scoring', exact: true })).toContainText('1. Spark');
  await page.locator('.trade-position').filter({ hasText: 'Skill and support gems' }).click();
  await page.getByRole('button', { name: 'Calculate affix scores', exact: true }).click();
  const replacement = page.locator('.trade-gem-card').filter({ hasText: 'Replaces Impending Doom' }).first();
  await expect(replacement).toBeVisible({ timeout: 90_000 });
  // The card shows "before → after"; the prediction is the value after the arrow.
  const predicted = Number((await replacement.locator('.upgrade-gem-stats > span').first().locator('b').last().innerText()).split('→').pop()!.replaceAll(',', ''));
  await replacement.getByRole('button', { name: 'Apply to build', exact: true }).click();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  const current = await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state);
  expect(current.params.main_socket_group).toBe(0);
  expect(current.socketGroups[0].main_active_skill).toBe(2);
  expect(current.socketGroups[0].weapon_set).toBe(2);
  // The best replacement for that gem depends on the game data (it used to be Rapid Casting); what matters is that it replaced it.
  expect(current.socketGroups[0].gems[1].skill_id).not.toBe('ViciousHexSupportPlayer');
  expect(current.socketGroups[0].gems[1].skill_id).toMatch(/^Support/);
  await expect(inspectedSkill(page)).toHaveText('1. Spark');
  const dps = dpsValue(page);
  await expect.poll(async () => Number((await dps.innerText()).replaceAll(',', ''))).toBeCloseTo(predicted, 0);
  await page.reload();
  await expect(inspectedSkill(page)).toHaveText('1. Spark', { timeout: 90_000 });
  await expect.poll(async () => Number((await dps.innerText()).replaceAll(',', ''))).toBeCloseTo(predicted, 0);
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state.socketGroups[0].main_active_skill)).toBe(2);
});
