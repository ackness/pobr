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
