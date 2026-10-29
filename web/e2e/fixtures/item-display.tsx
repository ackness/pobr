import { getBackend } from '../../src/api/backend';
import type { ItemLineJson } from '../../src/api/types';

// The real app, preferences, item view and session run against deterministic presentation data.
// This fixture tests UI behaviour, not the engine's affix lookup or calculation accuracy.
const lines: ItemLineJson[] = [
  { kind: 'name', text: 'Woe Rain' }, { kind: 'base', text: 'Seeker Bow' },
  { kind: 'struct', text: 'Item Level: 81' }, { kind: 'struct', text: 'LevelReq: 77' },
  { kind: 'struct', text: 'Dex: 163' }, { kind: 'struct', text: 'Quality: 20' },
  { kind: 'struct', text: 'Sockets: S S' }, { kind: 'struct', text: 'Corrupted' },
  { kind: 'struct', text: "Rune: Countess Seske's Rune of Archery" },
  { kind: 'struct', text: "Rune: Saqawal's Rune of the Sky" },
  { kind: 'enchant', text: '23% increased Elemental Damage with Attacks' },
  { kind: 'rune', text: 'Bow Attacks fire an additional Arrow' },
  { kind: 'rune', text: 'Gain 5% of Damage as Extra Damage of all Elements' },
  { kind: 'explicit', text: 'Adds 12 to 228 Lightning Damage', affix: 'prefix', tier: 1, tier_total: 9 },
  { kind: 'explicit', text: 'Adds 69 to 85 Cold Damage', affix: 'prefix', tier: 2, tier_total: 9 },
  { kind: 'explicit', text: '+4.92% to Critical Hit Chance' },
  { kind: 'explicit', text: '14% increased Attack Speed', affix: 'suffix', tier: 2, tier_total: 5 },
  { kind: 'explicit', text: '+2 to Level of all Attack Skills' },
  { kind: 'explicit', text: 'Adds 65 to 106 Fire Damage', affix: 'prefix', tier: 2, tier_total: 9 },
];
const text = 'Rarity: RARE\n' + lines.map(line => `${line.kind === 'enchant' || line.kind === 'rune' ? `{${line.kind}}` : ''}${line.text}`).join('\n');
const backend = await getBackend();
const decode = backend.decodeBuild;
backend.decodeBuild = async code => {
  const result = await decode(code);
  return { ...result, items: { ...result.items, equipped: [{ slot: 'weapon1', text }] } };
};
backend.classifyItemLines = async raw => raw === text ? lines : [];
const translations: Record<string, string> = {
  'Woe Rain': '灾祸 之雨', 'Seeker Bow': '寻战之弓',
  '23% increased Elemental Damage with Attacks': '攻击技能的元素伤害提高 23%',
  'Bow Attacks fire an additional Arrow': '弓类攻击发射一支额外箭矢',
  'Gain 5% of Damage as Extra Damage of all Elements': '获得相当于伤害 5% 的所有元素额外伤害',
  'Adds 12 to 228 Lightning Damage': '附加 12 - 228 基础闪电伤害',
  'Adds 69 to 85 Cold Damage': '附加 69 - 85 冰霜伤害',
  '+4.92% to Critical Hit Chance': '暴击几率 +4.92%',
  '14% increased Attack Speed': '攻击速度提高 14%',
  '+2 to Level of all Attack Skills': '所有攻击技能等级 +2',
  'Adds 65 to 106 Fire Damage': '附加 65 - 106 基础火焰伤害',
  "Countess Seske's Rune of Archery": '塞丝克伯爵夫人的箭术符文',
  "Saqawal's Rune of the Sky": '萨贾沃的密空符文',
};
backend.translateLines = async (source, lang) => source.map(line => lang === 'en-US' ? line : translations[line] ?? line);
const probe = { calculations: 0, text };
const calculate = backend.calculateBuild;
backend.calculateBuild = request => { probe.calculations++; return calculate(request); };
Object.assign(window, { itemDisplayProbe: probe });
history.replaceState(null, '', '/build');
await import('../../src/main');
