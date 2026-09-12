import { expect, test, type Page } from '@playwright/test';

const chest = [
  'Rarity: RARE', 'Local Defence Fixture', 'Chain Mail', 'Item Level: 50',
  'Quality: 20', 'Armour: 72', 'Evasion Rating: 72',
  'Sockets: S', 'Rune: Perfect Iron Rune', 'Implicits: 1',
  '{rune}20% increased Armour, Evasion and Energy Shield',
  '+25 to Armour', '+34 to Evasion Rating', '+80 to maximum Life',
].join('\n');

const bow = [
  'Rarity: RARE', 'Local Physical Fixture', 'Crude Bow', 'Item Level: 50',
  'Quality: 20', 'Note: Physical Damage: 14-26',
  'Sockets: S', 'Rune: Iron Rune', 'Implicits: 1',
  '{rune}16% increased Physical Damage',
  '50% increased Physical Damage', 'Adds 1 to 4 Physical Damage',
  'Gain 5 Life per enemy killed',
].join('\n');

const numeric = (value: string) => Number(value.replaceAll(',', ''));
const stat = (page: Page, name: string) => page.locator('.stat-row')
  .filter({ hasText: new RegExp(`^${name}(?=[0-9\\s])`) }).locator('dd');
const savedState = (page: Page) => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!).state);

async function loadBuild(page: Page) {
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: 85, class_name: 'Ranger' },
    equipments: [
      { inventoryId: 'Weapon', baseType: 'Crude Bow', frameType: 0 },
      { inventoryId: 'Weapon2', baseType: 'Crude Bow', name: 'Inactive Bow', frameType: 2,
        explicitMods: ['Adds 1 to 4 Physical Damage'] },
      { inventoryId: 'BodyArmour', baseType: 'Chain Mail', frameType: 0 },
    ],
    talent_tree: { hashes: [], quest_stats: [] }, jewel_data: '[]',
    skills: [{ baseType: 'Ice Shot', support: false, properties: [{ type: 5, values: [['16', 0]] }] }],
  } }));
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticLocalAffixFixture');
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 30_000 });
  await page.getByRole('navigation', { name: 'Main navigation' }).getByRole('button', { name: 'Upgrades', exact: true }).click();
}

async function compare(page: Page, text: string) {
  await page.getByRole('textbox', { name: 'Complete item text' }).fill(text);
  await page.getByRole('button', { name: 'Calculate replacement', exact: true }).click();
  await expect(page.locator('.replacement-content')).toHaveAttribute('aria-busy', 'false');
  await expect(page.getByRole('button', { name: /Apply to build/ })).toBeEnabled();
  await expect(page.locator('.replacement-unsupported')).toHaveCount(0);
}

async function equipFixture(page: Page, text: string, slot: string) {
  await loadBuild(page);
  await compare(page, text);
  await page.getByRole('button', { name: /Apply to build/ }).click();
  await expect(page.locator('.replacement-content')).toHaveCount(0);
  await expect.poll(async () => (await savedState(page)).items.find((item: { slot: string }) => item.slot === slot)?.text).toBe(text);
  await expect(page.getByRole('button', { name: 'Calculate replacement', exact: true })).toBeEnabled();
  await compare(page, text);
}

async function maximizeCopiedAffix(page: Page, id: string, expectedLine: string) {
  const editor = page.locator('.replacement-affix-editor');
  await editor.locator('summary').click();
  const row = editor.locator(`[data-affix-id="${id}"]`);
  await expect(row).toBeVisible();
  await row.getByRole('button', { name: 'Maximum', exact: true }).click();
  await expect(page.getByRole('button', { name: /Apply to build/ })).toBeDisabled();
  await expect(row).toContainText(expectedLine);
  await editor.getByRole('button', { name: 'Recalculate edited item', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'Complete item text' })).toHaveValue(new RegExp(expectedLine.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
  await expect(page.getByRole('button', { name: /Apply to build/ })).toBeEnabled();
  await expect(page.locator('.replacement-unsupported')).toHaveCount(0);
}

test('changing life preserves local armour and evasion, quality and the socketed rune through real recalculation', async ({ page }) => {
  // One pure flat-defence prefix and one life prefix, without a merged hybrid.
  // (Chain Mail 25/16 + flat 25/34) * quality 1.2 * rune 1.2 = 72/72.
  await equipFixture(page, chest, 'bodyarmour');
  const original = await savedState(page);
  const armour = await stat(page, 'Armour').innerText();
  const evasion = await stat(page, 'Evasion').innerText();
  const life = await stat(page, 'Life').innerText();
  expect(numeric(armour)).toBeGreaterThan(0);
  expect(numeric(evasion)).toBeGreaterThan(0);

  await maximizeCopiedAffix(page, 'equipment:IncreasedLife6', '+84 to maximum Life');
  const edited = await page.getByRole('textbox', { name: 'Complete item text' }).inputValue();
  expect(edited).toContain('+25 to Armour\n+34 to Evasion Rating');
  expect(edited).toContain('Quality: 20');
  expect(edited.match(/^Rune: Perfect Iron Rune$/gm)).toHaveLength(1);
  expect(edited.match(/^\{rune\}20% increased Armour, Evasion and Energy Shield$/gm)).toHaveLength(1);
  expect(edited).not.toMatch(/^(?:Armour|Evasion Rating):/m);
  expect((await savedState(page)).items).toEqual(original.items);
  await expect(stat(page, 'Armour')).toHaveText(armour);
  await expect(stat(page, 'Evasion')).toHaveText(evasion);
  await expect(stat(page, 'Life')).toHaveText(life);
  const expectedLife = numeric(await page.locator('.upgrade-replacement-result tbody tr')
    .filter({ hasText: /^Life(?=[0-9\s])/ }).locator('td').nth(1).innerText());
  expect(expectedLife).toBeGreaterThan(numeric(life));

  await page.getByRole('button', { name: /Apply to build/ }).click();
  await expect.poll(async () => numeric(await stat(page, 'Life').innerText())).toBeCloseTo(expectedLife, 0);
  await expect(stat(page, 'Armour')).toHaveText(armour);
  await expect(stat(page, 'Evasion')).toHaveText(evasion);
  const applied = await savedState(page);
  expect(applied.items.find((item: { slot: string }) => item.slot === 'bodyarmour').text).toBe(edited);
  expect(applied.items.filter((item: { slot: string }) => item.slot !== 'bodyarmour'))
    .toEqual(original.items.filter((item: { slot: string }) => item.slot !== 'bodyarmour'));
  expect(applied.weaponSwap).toEqual(original.weaponSwap);
});

test('changing life gained on kill preserves local physical damage, quality and Iron Rune DPS', async ({ page }) => {
  await equipFixture(page, bow, 'weapon1');
  const original = await savedState(page);
  const dps = await stat(page, 'Total DPS').innerText();
  expect(numeric(dps)).toBeGreaterThan(0);

  await maximizeCopiedAffix(page, 'equipment:LifeGainedFromEnemyDeath1', 'Gain 6 Life per enemy killed');
  const edited = await page.getByRole('textbox', { name: 'Complete item text' }).inputValue();
  expect(edited).toContain('50% increased Physical Damage\nAdds 1 to 4 Physical Damage');
  expect(edited).toContain('Quality: 20');
  expect(edited).toContain('Note: Physical Damage: 14-26');
  expect(edited.match(/^Rune: Iron Rune$/gm)).toHaveLength(1);
  expect(edited.match(/^\{rune\}16% increased Physical Damage$/gm)).toHaveLength(1);
  expect(edited).not.toContain('Gain 5 Life per enemy killed');
  expect((await savedState(page)).items).toEqual(original.items);
  await expect(stat(page, 'Total DPS')).toHaveText(dps);
  const expectedDps = numeric(await page.locator('.upgrade-replacement-result tbody tr').first().locator('td').nth(1).innerText());
  expect(expectedDps).toBeCloseTo(numeric(dps), 0);

  await page.getByRole('button', { name: /Apply to build/ }).click();
  await expect(page.locator('.replacement-content')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Calculate replacement', exact: true })).toBeEnabled();
  await expect(stat(page, 'Total DPS')).toHaveText(dps);
  const applied = await savedState(page);
  expect(applied.items.find((item: { slot: string }) => item.slot === 'weapon1').text).toBe(edited);
  expect(applied.items.filter((item: { slot: string }) => item.slot !== 'weapon1'))
    .toEqual(original.items.filter((item: { slot: string }) => item.slot !== 'weapon1'));
  expect(applied.weaponSwap).toEqual(original.weaponSwap);
});
