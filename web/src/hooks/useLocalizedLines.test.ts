import { expect, it, vi } from 'vitest';
import { cleanLine, itemLines, linesToTranslate, localizeStructLine, localizedLines, useLocalizedNames } from './useLocalizedLines';
import { itemDisplaySections } from '../lib/itemDisplay';
import { NativeNameIndex, type GameNameRequest } from '../lib/nativeDisplayNames';
import type { Lang } from '../lib/i18n';

const hooks = await vi.hoisted(async () => {
  const { createHookHarness } = await import('../testing/hookHarness');
  return createHookHarness({ memo: 'deps', effects: { comparison: 'objectIs', cleanup: 'previous' } });
});
const backend = vi.hoisted(() => ({ translateNames: vi.fn() }));
vi.mock('react', async original => hooks.mockReact(await original<typeof import('react')>()));
vi.mock('../api/backend', () => ({ getBackend: async () => backend }));

it.each([
  ['Armour', '护甲', '護甲'], ['Evasion', '闪避', '閃避'], ['Evasion Rating', '闪避值', '閃避值'],
  ['Energy Shield', '能量护盾', '能量護盾'], ['Ward', '结界', '結界'],
  ['Physical Damage', '物理伤害', '物理傷害'], ['Block Chance', '格挡几率', '格擋機率'],
])('localizes the %s item property only at the display boundary', (key, cn, tw) => {
  const original = `${key}: 431`;
  for (const [lang, label] of [['zh-CN', cn], ['zh-TW', tw], ['en-US', key]] as const) {
    const display = localizeStructLine(original, lang) ?? original;
    expect(display).toBe(`${label}: 431`);
    const lines = Object.freeze([Object.freeze({ kind: 'struct' as const, text: original })]);
    expect(itemDisplaySections(lines, [display]).properties).toEqual([{ label, value: '431', index: 0 }]);
    expect(itemDisplaySections(lines, [display]).metadata).toEqual([]);
    expect(lines[0].text).toBe(original);
  }
});

it('leaves modifier lines, unknown properties, numeric formatting and English source intact', () => {
  for (const lang of ['zh-CN', 'zh-TW', 'en-US'] as const) {
    expect(localizeStructLine('Future Defence: 431', lang)).toBeNull();
    expect(localizeStructLine('36% increased Evasion Rating', lang)).toBeNull();
  }
  expect(localizeStructLine('Evasion Rating: 1,234 (augmented)', 'zh-CN')).toBe('闪避值: 1,234 (augmented)');
  expect(localizeStructLine('Evasion Rating: 431', 'en-US')).toBeNull();
  expect(localizeStructLine('Attacks per Second: 1.25', 'zh-TW')).toBe('每秒攻擊次數: 1.25');
  expect(localizeStructLine('Radius: Large', 'zh-TW')).toBe('範圍: 大型');
  expect(localizeStructLine('Corrupted', 'zh-CN')).toBe('已腐化');
});

it('removes item markup and rarity headers from display lines', () => {
  const raw = 'Rarity: RARE\nExample\nEnergy Shield: 500\n{crafted}+10 to [MaximumLife|maximum Life]';
  expect(itemLines(raw)).toEqual(['Example', 'Energy Shield: 500', '+10 to maximum Life']);
  expect(cleanLine('{crafted}[Strength]')).toBe('Strength');
});

it('matches translations by position, so one line can be covered in one place and stand alone in another', () => {
  const lines = ['Item Level: 83', 'All Damage is taken from Mana before Life', '50% less Mana Recovery Rate', '', '50% less Mana Recovery Rate'];
  // Locally translated and blank lines are sent blank so they still separate their neighbours.
  expect(linesToTranslate(lines, 'zh-CN')).toEqual(['', lines[1], lines[2], '', lines[4]]);
  const out = [undefined, '受到的所有伤害在扣除生命前优先从魔力扣除', '', undefined, '魔力回复速度总降 50%'];
  expect(localizedLines(lines, 'zh-CN', out))
    .toEqual(['物品等级: 83', '受到的所有伤害在扣除生命前优先从魔力扣除', '', '', '魔力回复速度总降 50%']);
  expect(localizedLines(['Rune: Iron Rune'], 'zh-CN', [''])).toEqual(['符文: Iron Rune']);
});

it('returns the en-US lines themselves, so consumers memoizing on them do not recompute', () => {
  const lines = ['+5 to Strength', 'Item Level: 83'];
  expect(localizedLines(lines, 'en-US', [])).toBe(lines);
});

it('takes a Bonded line from the language pack, whole first, else around the pack\'s own prefix', () => {
  const lines = ['Bonded: +40 to maximum Life', 'Item Level: 83', 'Bonded: Grants Skill: Level 20 Foo', 'Bonded: Unknown words'];
  // Each Bonded line goes whole; its effect and the prefix probe follow for the fallback.
  const sent = linesToTranslate(lines, 'zh-CN');
  expect(sent).toEqual([lines[0], '', lines[2], lines[3],
    '', '+40 to maximum Life', '', 'Grants Skill: Level 20 Foo', '', 'Unknown words', '', 'Bonded: {0}']);
  const out = ['羁绊： +40 生命上限', undefined, lines[2], lines[3],
    undefined, '+40 生命上限', undefined, '获得技能：20 级 Foo', undefined, 'Unknown words', undefined, '羁绊： {0}'];
  expect(localizedLines(lines, 'zh-CN', out))
    .toEqual(['羁绊：+40 生命上限', '物品等级: 83', '羁绊：获得技能：20 级 Foo', 'Bonded: Unknown words']);
  // CN uses the chosen prefix even when a pack has no prefix template.
  expect(localizedLines([lines[2]], 'zh-CN', [lines[2], undefined, '获得技能：20 级 Foo', undefined, 'Bonded: {0}']))
    .toEqual(['羁绊：获得技能：20 级 Foo']);
  // A whole-line row that kept the English prefix gives way to the pack's own prefix around the effect…
  expect(localizedLines([lines[0]], 'zh-CN', ['Bonded: +40 生命上限', undefined, '+40 生命上限', undefined, '纽带：{0}']))
    .toEqual(['羁绊：+40 生命上限']);
  // …and is still shown when the pack cannot compose one (a partly translated line beats English).
  expect(localizedLines([lines[0]], 'zh-TW', ['Bonded: 生命上限 +40'])).toEqual(['Bonded: 生命上限 +40']);
  expect(localizedLines([lines[0]], 'zh-CN', [])).toEqual([lines[0]]);
  expect(linesToTranslate(['+5 to Strength'], 'zh-CN')).toEqual(['+5 to Strength']);
});

it('merges a wrapped Bonded effect before whole-line and fallback translation, preserving row positions', () => {
  const head = 'Bonded: 12% chance when collecting an Elemental Infusion to gain an';
  const tail = 'additional Elemental Infusion of the same type';
  const lines = [head, tail, 'Bonded: Archon recovery period expires 30% faster', '+40 to maximum Life'];
  const sent = linesToTranslate(lines, 'zh-CN');
  expect(sent[0]).toBe(`${head} ${tail}`);
  expect(sent[1]).toBe('');
  expect(sent[5]).toBe(`${head.replace('Bonded: ', '')} ${tail}`);
  const cn = localizedLines(lines, 'zh-CN', ['羁绊：收集元素灌注时有 12% 的几率额外获得一个同类元素灌注', undefined, '纽带：执政官恢复期的结束速度加快 30%', '+40 生命上限']);
  expect(cn).toEqual(['羁绊：收集元素灌注时有 12% 的几率额外获得一个同类元素灌注', '', '羁绊：执政官恢复期的结束速度加快 30%', '+40 生命上限']);
  expect(localizedLines(lines, 'en-US', [])[0]).toBe(`${head} ${tail}`);
  expect(localizedLines([lines[2]], 'zh-TW', ['連結：恢復期加快 30%'])).toEqual(['連結：恢復期加快 30%']);
  expect(lines).toEqual([head, tail, lines[2], '+40 to maximum Life']);
  expect(linesToTranslate(['Bonded: +40 to maximum Life', '+50 to maximum Mana'], 'zh-CN').slice(0, 2))
    .toEqual(['Bonded: +40 to maximum Life', '+50 to maximum Mana']);
});

it('keys localized class names by id and language, keeping fallbacks on failed and superseded requests', async () => {
  hooks.reset();
  backend.translateNames.mockReset();
  const calls: { resolve: (names: string[]) => void; reject: (error: Error) => void }[] = [];
  backend.translateNames.mockImplementation(() => new Promise<string[]>((resolve, reject) => calls.push({ resolve, reject })));
  const first: GameNameRequest[] = [{ kind: 'itemClass', id: 'First', name: 'Same' }];
  const second: GameNameRequest[] = [{ kind: 'itemClass', id: 'Second', name: 'Same' }];
  const render = (requests: GameNameRequest[], lang: Lang) => {
    hooks.beginRender();
    const result = useLocalizedNames(requests, lang);
    hooks.runEffects();
    return result;
  };
  expect(render(first, 'zh-CN')).toEqual(['Same']);
  await vi.waitFor(() => expect(calls).toHaveLength(1));
  calls[0].resolve(['类别甲']);
  await vi.waitFor(() => expect(render(first, 'zh-CN')).toEqual(['类别甲']));
  expect(render(second, 'zh-CN')).toEqual(['Same']);
  await vi.waitFor(() => expect(calls).toHaveLength(2));
  expect(render(second, 'zh-TW')).toEqual(['Same']);
  await vi.waitFor(() => expect(calls).toHaveLength(3));
  calls[2].reject(new Error('display member unavailable'));
  calls[1].resolve(['must not appear in another language']);
  await Promise.allSettled(backend.translateNames.mock.results.map(result => result.value));
  expect(hooks.dirty).toBe(false);
  expect(render(second, 'zh-TW')).toEqual(['Same']);
  expect(render(second, 'en-US')).toEqual(['Same']);
});

it('separates legacy class names from explicit undefined, empty and wrong ids without changing requests', async () => {
  hooks.reset();
  backend.translateNames.mockReset();
  const index = new NativeNameIndex([], [], { Ring: 'Rings' });
  backend.translateNames.mockImplementation(async (requests: GameNameRequest[]) =>
    index.split(requests, 'zh-CN').merge(['共享旧类别']));
  const requests: GameNameRequest[][] = [
    [{ kind: 'itemClass', name: 'Rings' }],
    [{ kind: 'itemClass', id: undefined, name: 'Rings' }],
    [{ kind: 'itemClass', id: '', name: 'Rings' }],
    [{ kind: 'itemClass', id: 'Unknown', name: 'Rings' }],
  ];
  const render = (names: GameNameRequest[]) => {
    hooks.beginRender();
    const result = useLocalizedNames(names, 'zh-CN');
    hooks.runEffects();
    return result;
  };
  for (const [position, names] of requests.entries()) {
    expect(render(names)).toEqual(['Rings']);
    await vi.waitFor(() => expect(backend.translateNames).toHaveBeenCalledTimes(position + 1));
    expect(backend.translateNames.mock.lastCall![0]).toBe(names);
    await vi.waitFor(() => expect(render(names)).toEqual([position === 0 ? '共享旧类别' : 'Rings']));
  }
  expect(Object.hasOwn(requests[0][0], 'id')).toBe(false);
  expect(Object.hasOwn(requests[1][0], 'id')).toBe(true);
  expect(requests[1][0]).toStrictEqual({ kind: 'itemClass', id: undefined, name: 'Rings' });
  expect(render(requests[0])).toEqual(['Rings']);
  await vi.waitFor(() => expect(backend.translateNames).toHaveBeenCalledTimes(5));
  await vi.waitFor(() => expect(render(requests[0])).toEqual(['共享旧类别']));
});
