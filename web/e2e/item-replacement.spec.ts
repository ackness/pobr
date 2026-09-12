import { expect, test, type Page } from '@playwright/test';

async function loadRings(page: Page) {
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: 85, class_name: 'Sorceress' },
    equipments: [
      { inventoryId: 'Weapon', baseType: 'Crude Bow', frameType: 0 },
      { inventoryId: 'Weapon2', baseType: 'Ashen Staff', name: 'Alternate Staff', frameType: 2, explicitMods: ['40% increased Spell Damage'] },
      { inventoryId: 'Ring', baseType: 'Sapphire Ring', name: 'Strong Ring', frameType: 2, explicitMods: ['60% increased Fire Damage', '+80 to maximum Life'] },
      { inventoryId: 'Ring2', baseType: 'Sapphire Ring', name: 'Weak Ring', frameType: 2, explicitMods: ['+20 to maximum Life'] },
    ],
    talent_tree: { hashes: [], quest_stats: [] }, jewel_data: '[]',
    skills: [{ baseType: 'Fireball', support: false, properties: [{ type: 5, values: [['16', 0]] }] }],
  } }));
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticCopiedItemFixture');
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 30_000 });
  await page.getByRole('navigation', { name: 'Main navigation' }).getByRole('button', { name: 'Upgrades', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'Complete item text' })).toBeVisible();
}

const dps = (page: Page) => page.locator('.stat-row').filter({ hasText: 'Total DPS' }).locator('dd');
const numeric = (text: string) => Number(text.replaceAll(',', ''));
const savedState = (page: Page) => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!).state);
const copied = 'Item Class: Rings\nRarity: RARE\nMarket Candidate\nSapphire Ring\n--------\nRequirements:\nLevel: 60\nDex: 90\n--------\nItem Level: 100\nImplicits: 1\n+30% to Cold Resistance\n--------\n20% increased Fire Damage\n+100 to maximum Life';

async function paste(page: Page, text: string) {
  await page.getByRole('textbox', { name: 'Complete item text' }).evaluate((element, value) => {
    const data = new DataTransfer(); data.setData('text/plain', value);
    element.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }));
  }, text);
}

test('pasting detects both rings, recommends the actual better destination, and applies only the selected replacement', async ({ page }) => {
  await loadRings(page);
  const before = await dps(page).innerText();
  const original = await savedState(page);
  await paste(page, copied);
  const destinations = page.locator('.replacement-position');
  await expect(destinations).toHaveCount(2);
  const best = destinations.filter({ hasText: /^Ring 2/ });
  await expect(best).toHaveAttribute('aria-pressed', 'true');
  await expect(best).toContainText('Best for your goal');
  await expect(page.locator('.replacement-affix')).toHaveCount(3);
  await expect(page.locator('.replacement-affixes')).not.toContainText('Item Level');
  await expect(page.locator('.replacement-affixes')).not.toContainText('Dex:');
  await expect(dps(page)).toHaveText(before);
  expect((await savedState(page)).items).toEqual(original.items);
  await destinations.filter({ hasText: /^Ring 1/ }).click();
  await expect(page.locator('.upgrade-replacement-result')).toContainText('DPS decreases after this replacement');
  await best.click();
  const expectedDps = numeric(await page.locator('.upgrade-replacement-result tbody tr').first().locator('td').nth(1).innerText());
  await page.getByRole('button', { name: 'Apply to build · Ring 2', exact: true }).click();
  await expect(page.locator('.upgrade-replacement-result')).toHaveCount(0);
  await expect.poll(async () => numeric(await dps(page).innerText())).toBeCloseTo(expectedDps, 0);
  const applied = await savedState(page);
  expect(applied.items.find((item: { slot: string }) => item.slot === 'ring1')).toEqual(original.items.find((item: { slot: string }) => item.slot === 'ring1'));
  expect(applied.items.find((item: { slot: string }) => item.slot === 'ring2').text).toContain('Market Candidate');
  expect(applied.weaponSwap).toEqual(original.weaponSwap);
});

test('a complete Chinese market copy matches the English calculation and weapon changes invalidate its result', async ({ page }) => {
  await loadRings(page);
  await paste(page, copied);
  await expect(page.locator('.replacement-position')).toHaveCount(2);
  const englishDps = await page.locator('.upgrade-replacement-result tbody tr').first().locator('td').nth(1).innerText();
  const cn = '物品种类: 戒指\n稀有度：稀有\n市集候选\n蓝玉戒指\n--------\n需求：等级60，90敏捷\n--------\n物品等级: 100\nImplicits: 1\n冰霜抗性 +30%\n--------\n火焰伤害提高 20%\n+100 生命上限';
  await paste(page, cn);
  await expect(page.locator('.replacement-parsed')).toContainText('市集候选');
  await expect(page.locator('.replacement-position')).toHaveCount(2);
  await expect(page.locator('.upgrade-replacement-result tbody tr').first().locator('td').nth(1)).toHaveText(englishDps);
  await expect(page.locator('.replacement-unsupported')).toHaveCount(0);
  const original = await savedState(page);
  await page.getByRole('group', { name: 'Active weapon set' }).getByRole('button', { name: 'Set 2', exact: true }).click();
  await expect(page.locator('.upgrade-replacement-result')).toHaveCount(0);
  await expect(page.getByRole('button', { name: /Apply to build/ })).toHaveCount(0);
  expect((await savedState(page)).items.filter((item: { slot: string }) => item.slot.startsWith('ring')))
    .toEqual(original.items.filter((item: { slot: string }) => item.slot.startsWith('ring')));
});

test('unknown effects remain visible and impossible copies explain why they cannot be compared', async ({ page }) => {
  await loadRings(page);
  await paste(page, `${copied}\nUnmodeled synthetic effect`);
  await expect(page.locator('.replacement-unsupported')).toContainText('Unmodeled synthetic effect');
  await expect(page.locator('.replacement-recommended')).toHaveCount(0);
  const cases = [
    [copied.replace('Level: 60', 'Level: 90'), 'Required level exceeds'],
    ['Rarity: NORMAL\nRuby', 'No compatible equipped position'],
    [`${copied}\nUnidentified`, 'Identify the item'],
    ['Rarity: NORMAL\nAshen Staff', 'requires a coordinated'],
  ];
  for (const [text, message] of cases) {
    await page.getByRole('textbox', { name: 'Complete item text' }).fill(text);
    await page.getByRole('button', { name: 'Calculate replacement', exact: true }).click();
    await expect(page.locator('.upgrade-item-check').getByRole('alert')).toContainText(message);
    await expect(page.locator('.upgrade-replacement-result')).toHaveCount(0);
  }
});

async function loadAugmentedBuild(page: Page, extraEquipment: unknown[] = []) {
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: 85, class_name: 'Ranger' },
    equipments: [
      { inventoryId: 'Weapon', baseType: 'Crude Bow', name: 'Current Bow', frameType: 2,
        sockets: [{ type: 'rune' }], socketedItems: [{ socket: 0, baseType: 'Perfect Iron Rune' }],
        runeMods: ['20% increased Physical Damage'], explicitMods: ['Adds 20 to 40 Physical Damage'] },
      { inventoryId: 'Weapon2', baseType: 'Crude Bow', name: 'Alternate Bow', frameType: 2,
        explicitMods: ['Adds 10 to 20 Physical Damage'] },
      { inventoryId: 'BodyArmour', baseType: 'Leather Vest', name: 'Current Chest', frameType: 2,
        sockets: [{ type: 'rune' }], socketedItems: [{ socket: 0, baseType: 'Perfect Body Rune' }],
        runeMods: ['+75 to maximum Life'], explicitMods: ['+50 to maximum Life'] },
      ...extraEquipment,
    ],
    talent_tree: { hashes: [], quest_stats: [] }, jewel_data: '[]',
    skills: [{ baseType: 'Ice Shot', support: false, properties: [{ type: 5, values: [['16', 0]] }] }],
  } }));
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticAugmentFixture');
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 30_000 });
  await page.getByRole('navigation', { name: 'Main navigation' }).getByRole('button', { name: 'Upgrades', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'Complete item text' })).toBeVisible();
}

const bareBow = 'Rarity: RARE\nMarket Bow\nCrude Bow\n--------\nImplicits: 0\nAdds 20 to 40 Physical Damage';
const bareChest = 'Rarity: RARE\nMarket Chest\nLeather Vest\n--------\nImplicits: 0\n+50 to maximum Life';

test('shared augment limits include other equipment and anonymous effects never receive a verified recommendation', async ({ page }) => {
  await loadAugmentedBuild(page, [{ inventoryId: 'Helm', baseType: 'Chain Tiara', name: 'Limited Helmet', frameType: 2,
    sockets: [{ type: 'rune' }], socketedItems: [{ socket: 0, baseType: "Jiquani's Thesis" }],
    runeMods: ['+1 to maximum Mana per 3 Item Armour on Equipped Helmet'] }]);
  const original = await savedState(page);
  await paste(page, `${bareChest}\nSockets: S\nRune: Amanamu's Gaze\n{rune}+2 to Armour per 1 Spirit`);
  await expect(page.locator('.upgrade-item-check').getByRole('alert')).toContainText('exceeds an equipped augment limit');
  await expect(page.locator('.replacement-content')).toHaveCount(0);
  await paste(page, `${bareChest}\nSockets: S\n{rune}+500 to maximum Life`);
  await expect(page.locator('.replacement-content')).toBeVisible();
  await expect(page.locator('.replacement-verdict')).toContainText('augment limits not verified');
  await expect(page.locator('.replacement-recommended')).toHaveCount(0);
  expect((await savedState(page)).items).toEqual(original.items);
});
const ehp = (page: Page) => page.locator('.stat-row').filter({ hasText: 'Effective HP' }).locator('dd');
const afterStat = async (page: Page, row: number) => numeric(await page.locator('.upgrade-replacement-result tbody tr').nth(row).locator('td').nth(1).innerText());

async function settledComparison(page: Page) {
  await expect(page.locator('.replacement-content')).toHaveAttribute('aria-busy', 'false');
  await expect(page.getByRole('button', { name: /Apply to build/ })).toBeEnabled();
}

async function selectAugment(page: Page, socket: number, name: string) {
  await page.getByRole('button', { name: `Socket ${socket}`, exact: true }).click();
  await page.locator(`[role="option"][data-value="${name}"]`).click();
  await settledComparison(page);
  await expect(page.getByRole('button', { name: `Socket ${socket}`, exact: true })).toContainText(name || 'Empty socket');
}

test('bow comparison inherits current runes, permits socket simulation and applies the exact evaluated setup', async ({ page }) => {
  await loadAugmentedBuild(page);
  const original = await savedState(page);
  const originalDps = await dps(page).innerText();
  await paste(page, `${bareBow}\nUnmodeled market effect (pseudoMods): Sum: 369.1`);
  await settledComparison(page);
  await expect(page.locator('.replacement-augment-summary')).toContainText('Perfect Iron Rune');
  await expect(page.locator('.replacement-augment-assumption')).toContainText('Additional sockets assumed');
  await expect(page.locator('.replacement-affixes')).not.toContainText('Sum:');
  await expect(page.locator('.replacement-unsupported')).toHaveCount(0);
  const inheritedDps = await afterStat(page, 0);
  await page.getByRole('button', { name: 'As listed', exact: true }).click();
  await settledComparison(page);
  const rawDps = await afterStat(page, 0);
  expect(inheritedDps).toBeGreaterThan(rawDps);
  await expect(dps(page)).toHaveText(originalDps);
  expect((await savedState(page)).items).toEqual(original.items);

  await page.getByRole('button', { name: 'Use current setup', exact: true }).click();
  await settledComparison(page);
  await page.getByRole('button', { name: 'Customize', exact: true }).click();
  await settledComparison(page);
  await page.getByRole('button', { name: 'Add socket', exact: true }).click();
  await settledComparison(page);
  await selectAugment(page, 2, 'Iron Rune');
  const twoRuneDps = await afterStat(page, 0);
  expect(twoRuneDps).toBeGreaterThan(inheritedDps);
  await selectAugment(page, 1, '');
  const expectedDps = await afterStat(page, 0);
  expect(expectedDps).toBeGreaterThan(rawDps);
  expect(expectedDps).toBeLessThan(inheritedDps);
  expect((await savedState(page)).items).toEqual(original.items);
  await page.getByRole('button', { name: /Apply to build/ }).click();
  await expect(page.locator('.upgrade-replacement-result')).toHaveCount(0);
  await expect.poll(async () => numeric(await dps(page).innerText())).toBeCloseTo(expectedDps, 0);
  const applied = await savedState(page);
  const weapon = applied.items.find((item: { slot: string }) => item.slot === 'weapon1').text;
  expect(weapon).toContain('Market Bow');
  expect(weapon).toContain('Rune: Iron Rune');
  expect(weapon).not.toContain('Perfect Iron Rune');
  expect(applied.items.filter((item: { slot: string }) => item.slot !== 'weapon1'))
    .toEqual(original.items.filter((item: { slot: string }) => item.slot !== 'weapon1'));
  expect(applied.weaponSwap).toEqual(original.weaponSwap);
});

test('armour comparison includes life augments in EHP and preserves empty sockets when applying a manual setup', async ({ page }) => {
  await loadAugmentedBuild(page);
  const original = await savedState(page);
  const originalEhp = await ehp(page).innerText();
  await paste(page, bareChest);
  await settledComparison(page);
  await expect(page.locator('.replacement-augment-summary')).toContainText('Perfect Body Rune');
  const inheritedEhp = await afterStat(page, 1);
  await page.getByRole('button', { name: 'As listed', exact: true }).click();
  await settledComparison(page);
  const rawEhp = await afterStat(page, 1);
  expect(inheritedEhp).toBeGreaterThan(rawEhp);
  await page.getByRole('button', { name: 'Use current setup', exact: true }).click();
  await settledComparison(page);
  await page.getByRole('button', { name: 'Customize', exact: true }).click();
  await settledComparison(page);
  await page.getByRole('button', { name: 'Add socket', exact: true }).click();
  await settledComparison(page);
  await page.getByRole('searchbox', { name: 'Filter by name or effect' }).fill('maximum life');
  await selectAugment(page, 2, 'Body Rune');
  expect(await afterStat(page, 1)).toBeGreaterThan(inheritedEhp);
  await selectAugment(page, 1, '');
  const expectedEhp = await afterStat(page, 1);
  expect(expectedEhp).toBeGreaterThan(rawEhp);
  expect(expectedEhp).toBeLessThan(inheritedEhp);
  await expect(ehp(page)).toHaveText(originalEhp);
  expect((await savedState(page)).items).toEqual(original.items);
  await page.getByRole('button', { name: /Apply to build/ }).click();
  await expect(page.locator('.upgrade-replacement-result')).toHaveCount(0);
  await expect.poll(async () => numeric(await ehp(page).innerText())).toBeCloseTo(expectedEhp, 0);
  const applied = await savedState(page);
  const chest = applied.items.find((item: { slot: string }) => item.slot === 'bodyarmour').text;
  expect(chest).toContain('Market Chest');
  expect(chest).toContain('Rune: Body Rune');
  expect(chest).not.toContain('Perfect Body Rune');
  expect(applied.items.filter((item: { slot: string }) => item.slot !== 'bodyarmour'))
    .toEqual(original.items.filter((item: { slot: string }) => item.slot !== 'bodyarmour'));
});

test('repasting the same listing clears custom augments and build or input edits invalidate the comparison', async ({ page }) => {
  await loadAugmentedBuild(page);
  const original = await savedState(page);
  await paste(page, bareBow);
  await settledComparison(page);
  const inheritedDps = await afterStat(page, 0);
  await page.getByRole('button', { name: 'Customize', exact: true }).click();
  await settledComparison(page);
  await selectAugment(page, 1, '');
  expect(await afterStat(page, 0)).toBeLessThan(inheritedDps);
  await paste(page, bareBow);
  await settledComparison(page);
  await expect(page.getByRole('button', { name: 'Use current setup', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('.replacement-augment-summary')).toContainText('Perfect Iron Rune');
  await expect(page.getByRole('button', { name: 'Socket 1', exact: true })).toHaveCount(0);
  expect(await afterStat(page, 0)).toEqual(inheritedDps);
  await page.getByRole('button', { name: 'Calculate replacement', exact: true }).click();
  await settledComparison(page);
  expect(await afterStat(page, 0)).toEqual(inheritedDps);
  await page.getByRole('textbox', { name: 'Complete item text' }).fill(`${bareBow}\n+10 to maximum Mana`);
  await expect(page.locator('.upgrade-replacement-result')).toHaveCount(0);
  await expect(page.getByRole('button', { name: /Apply to build/ })).toHaveCount(0);
  await paste(page, bareBow);
  await settledComparison(page);
  await page.getByRole('group', { name: 'Active weapon set' }).getByRole('button', { name: 'Set 2', exact: true }).click();
  await expect(page.locator('.upgrade-replacement-result')).toHaveCount(0);
  await expect(page.getByRole('button', { name: /Apply to build/ })).toHaveCount(0);
  const after = await savedState(page);
  expect([...after.items, ...(after.weaponSwap?.alternate_items ?? [])].some((item: { text: string }) => item.text.includes('Market Bow'))).toBe(false);
  expect(after.items.filter((item: { slot: string }) => item.slot === 'bodyarmour'))
    .toEqual(original.items.filter((item: { slot: string }) => item.slot === 'bodyarmour'));
});
