import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';

const script = readFileSync(new URL('../public/userscripts/pobr-market-copy.user.js', import.meta.url), 'utf8');
const id = 'a'.repeat(64), otherId = 'b'.repeat(64);
const market = 'https://poe.game.qq.com/trade2/search/poe2/TestLeague/TestQuery';
const row = (key: string) => `<div class="row" data-id="${key}"><div class="item-popup"></div><div class="right"><div class="details"><button class="contact">Contact seller</button></div></div></div>`;
const ring = { frameType: 2, name: '市集候选', baseType: '蓝玉戒指', ilvl: 100, identified: true,
  requirements: [{ name: '等级', type: 62, values: [['60', 0]] }, { name: '[Dexterity|敏捷]', type: 64, values: [['90', 0]] }],
  implicitMods: [{ description: '[Resistances|冰霜抗性] +30%', mods: [{ tier: 'T1', name: 'Do not copy this tier' }] }],
  explicitMods: [{ description: '火焰伤害提高 20%' }, { description: '+100 生命上限' }],
};
const copied = (page: Page) => page.evaluate(() => (window as unknown as { copies: string[] }).copies);
async function setup(page: Page, item: object = ring, url = market, clipboardFails = false) {
  let fetches = 0;
  await page.route('https://poe.game.qq.com/**', route => route.fulfill({ contentType: 'text/html', body: `<div class="resultset">${row(id)}</div>` }));
  await page.route('https://www.pathofexile.com/**', route => route.fulfill({ contentType: 'text/html', body: `<div class="resultset">${row(id)}</div>` }));
  await page.route('**/api/trade2/fetch/**', route => {
    fetches++;
    const request = new URL(route.request().url());
    expect(request.pathname).toBe(`/api/trade2/fetch/${id}`);
    expect(request.searchParams.get('query')).toBe('TestQuery');
    expect(request.searchParams.get('realm')).toBe('poe2');
    return route.fulfill({ json: { result: [{ id, item, listing: { account: { name: 'PRIVATE_SELLER' }, whisper: 'PRIVATE_WHISPER', price: { amount: 50 } } }] } });
  });
  await page.goto(url);
  await page.evaluate(fails => {
    Object.assign(window, { copies: [], contacts: 0, GM: { setClipboard: async (text: string) => {
      if (fails) throw new Error('Clipboard denied');
      (window as unknown as { copies: string[] }).copies.push(text);
    } } });
    document.querySelector('.contact')!.addEventListener('click', () => (window as unknown as { contacts: number }).contacts++);
  }, clipboardFails);
  await page.addScriptTag({ content: script });
  return () => fetches;
}

test('the userscript reads only a clicked item and preserves full CN modifiers without seller data or roll labels', async ({ page }) => {
  const count = await setup(page, { ...ring, sockets: [{ type: 'rune' }, { type: 'rune' }],
    socketedItems: [{ socket: 0, baseType: 'Storm Rune' }], runeMods: ['+14% to Lightning Resistance'],
    enchantMods: ['+20 to maximum Mana'], craftedMods: ['+10 to Strength'], fracturedMods: ['+15 to Dexterity'],
    unknownMods: ['100% more Damage'] });
  expect(count()).toBe(0);
  await page.getByRole('button', { name: '复制到 PoBR', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('已复制');
  const [text] = await copied(page);
  expect(text).toContain('Rarity: RARE\n市集候选\n蓝玉戒指');
  expect(text).toContain('Requirements:\nLevel: 60\nDex: 90');
  expect(text).toContain('Item Level: 100');
  expect(text).toContain('Sockets: S S\nRune: Storm Rune');
  expect(text).not.toContain('Rune: None');
  expect(text).toContain('Implicits: 1\n冰霜抗性 +30%');
  expect(text).toContain('{rune}+14% to Lightning Resistance');
  expect(text).toContain('{enchant}+20 to maximum Mana');
  expect(text).toContain('{crafted}+10 to Strength');
  expect(text).toContain('{fractured}+15 to Dexterity');
  expect(text).toContain('Unmodeled market effect (unknownMods): 100% more Damage');
  expect(text).not.toMatch(/PRIVATE_|T1|Do not copy/);
  expect(count()).toBe(1);
  expect(await page.evaluate(() => (window as unknown as { contacts: number }).contacts)).toBe(0);
});

test('international magic copies keep an exact base, decorated name metadata and rolled defences', async ({ page }) => {
  await setup(page, { frameType: 1, baseType: 'Chain Tiara', typeLine: 'Healthy Chain Tiara of the Storm', ilvl: 80,
    properties: [{ type: 6, values: [['+20%', 0]] }, { type: 18, values: [['123', 0]] }],
    explicitMods: ['+50 to maximum Life'], corrupted: true }, 'https://www.pathofexile.com/trade2/search/TestLeague/TestQuery');
  await page.getByRole('button', { name: 'Copy for PoBR' }).click();
  await expect(page.getByRole('status')).toContainText('Copied');
  const [text] = await copied(page);
  expect(text).toMatch(/^Rarity: MAGIC\nChain Tiara\n--------/);
  expect(text).toContain('Note: Healthy Chain Tiara of the Storm');
  expect(text).toContain('Quality: +20%\nEnergy Shield: 123');
  expect(text).toContain('Corrupted');
});

test('utility descriptions interpolate API placeholders and preserve requirements as metadata', async ({ page }) => {
  await setup(page, { frameType: 1, baseType: 'Ultimate Mana Flask',
    properties: [{ name: '{1} 秒内回复 {0} 魔力', displayMode: 3, values: [['500', 1], ['4', 0]] }],
    requirements: [{ name: 'Dexterity', values: [['80', 0]] }, { name: 'Class', values: [['Test Class', 0]] }],
    explicitMods: ['20% increased Charges gained'] });
  await page.getByRole('button', { name: '复制到 PoBR', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('已复制');
  const [text] = await copied(page);
  expect(text).toContain('Note: 4 秒内回复 500 魔力');
  expect(text).toContain('Dex: 80\nNote: Requirement - Class: Test Class');
  expect(text).not.toMatch(/\{[01]\}/);
});

test('newly loaded rows receive one button and a changed row cannot copy an old in-flight response', async ({ page }) => {
  await setup(page);
  await page.addScriptTag({ content: script });
  await expect(page.getByRole('button', { name: '复制到 PoBR', exact: true })).toHaveCount(1);
  await page.locator('.resultset').evaluate((element, html) => element.insertAdjacentHTML('beforeend', html), row(otherId));
  await expect(page.getByRole('button', { name: '复制到 PoBR', exact: true })).toHaveCount(2);
  let release!: () => void;
  const delayed = new Promise<void>(resolve => { release = resolve; });
  let started!: () => void;
  const requested = new Promise<void>(resolve => { started = resolve; });
  await page.route('**/api/trade2/fetch/**', async route => {
    started(); await delayed;
    await route.fulfill({ json: { result: [{ id, item: ring }] } });
  });
  await page.getByRole('button', { name: '复制到 PoBR', exact: true }).first().click();
  await requested;
  await page.locator('.row').first().evaluate((element, value) => { (element as HTMLElement).dataset.id = value; }, otherId);
  release();
  await expect(page.getByRole('status').first()).toContainText('搜索结果已变化');
  expect(await copied(page)).toEqual([]);
});

test('rate limits and mismatched results never retry or overwrite the clipboard', async ({ page }) => {
  await setup(page);
  let requests = 0;
  await page.route('**/api/trade2/fetch/**', route => { requests++; return route.fulfill({ status: 429, json: { error: 'rate limit' } }); });
  await page.getByRole('button', { name: '复制到 PoBR', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('限流');
  expect(requests).toBe(1); expect(await copied(page)).toEqual([]);
  await page.route('**/api/trade2/fetch/**', route => route.fulfill({ json: { result: [{ id: otherId, item: ring }] } }));
  await page.getByRole('button', { name: '复制到 PoBR', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('商品已下架');
  expect(await copied(page)).toEqual([]);
});

test('clipboard denial opens selected complete text for manual copying', async ({ page }) => {
  await setup(page, ring, market, true);
  await page.getByRole('button', { name: '复制到 PoBR', exact: true }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await expect(page.getByRole('textbox', { name: '完整物品文本' })).toHaveValue(/Rarity: RARE/);
  const selected = await page.getByRole('textbox').evaluate(element => {
    const input = element as HTMLTextAreaElement;
    return input.selectionEnd - input.selectionStart === input.value.length;
  });
  expect(selected).toBe(true); expect(await copied(page)).toEqual([]);
  await page.getByRole('button', { name: '关闭', exact: true }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
});

test('a delayed clipboard failure cannot open a manual dialog for a removed listing', async ({ page }) => {
  await setup(page);
  await page.evaluate(() => {
    Object.assign(window, { GM: { setClipboard: () => new Promise((_resolve, reject) => {
      Object.assign(window, { rejectCopy: () => reject(new Error('Clipboard denied')) });
    }) } });
  });
  await page.getByRole('button', { name: '复制到 PoBR', exact: true }).click();
  await page.waitForFunction(() => 'rejectCopy' in window);
  await page.evaluate(async () => {
    document.querySelector('.row')!.remove();
    (window as unknown as { rejectCopy: () => void }).rejectCopy();
    await new Promise(resolve => setTimeout(resolve, 0));
  });
  await expect(page.getByRole('dialog')).toHaveCount(0);
  expect(await copied(page)).toEqual([]);
});

test('text copied from a market response reaches real WASM replacement comparison', async ({ page }) => {
  await setup(page);
  await page.getByRole('button', { name: '复制到 PoBR', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('已复制');
  const [text] = await copied(page);
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: 85, class_name: 'Sorceress' },
    equipments: [{ inventoryId: 'Ring', baseType: 'Sapphire Ring', frameType: 2, name: 'Current Ring', explicitMods: ['40% increased Fire Damage', '+80 to maximum Life'] }],
    talent_tree: { hashes: [], quest_stats: [] }, jewel_data: '[]',
    skills: [{ baseType: 'Fireball', support: false, properties: [{ type: 5, values: [['16', 0]] }] }],
  } }));
  await page.goto('/');
  await page.getByRole('textbox', { name: 'Build code' }).fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticUserscriptFixture');
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 30000 });
  await page.getByRole('navigation', { name: 'Main navigation' }).getByRole('button', { name: 'Upgrades', exact: true }).click();
  await page.getByRole('textbox', { name: 'Complete item text' }).evaluate((element, value) => {
    const data = new DataTransfer(); data.setData('text/plain', value);
    element.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }));
  }, text);
  await expect(page.locator('.replacement-parsed')).toContainText('Required level 60');
  await expect(page.locator('.replacement-affix')).toHaveCount(3);
  await expect(page.locator('.replacement-unsupported')).toHaveCount(0);
  await expect(page.locator('.upgrade-replacement-result')).toContainText('DPS decreases after this replacement');
  await expect(page.locator('.replacement-copy-links a').first()).toHaveAttribute('href', '/userscripts/pobr-market-copy.user.js');
  const popup = page.waitForEvent('popup');
  await page.locator('.replacement-copy-links a').last().click();
  const guide = await popup;
  await expect(guide.locator('.install')).toBeVisible();
  const download = await guide.request.get(new URL(await guide.locator('.install').getAttribute('href') ?? '', guide.url()).href);
  expect(await download.text()).toBe(script);
});
