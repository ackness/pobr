import type { ItemLineJson } from '../api/types';
import type { Lang } from './i18n';
export { DISPLAY_MODES as ITEM_DISPLAY_STYLES } from './displayModes';
export type { DisplayMode as ItemDisplayStyle } from './displayModes';

export interface DisplayItemLine extends ItemLineJson { original: string; index: number }
export interface ItemProperty { label: string; value: string; index: number }

/** Presentation only: never infer an affix's identity or capacity from its position or line count. */
export function itemDisplaySections(lines: readonly ItemLineJson[], translated: readonly string[]) {
  const result = {
    name: '', bases: [] as DisplayItemLine[], states: [] as DisplayItemLine[],
    properties: [] as ItemProperty[], metadata: [] as DisplayItemLine[], runes: [] as DisplayItemLine[],
    effects: [] as DisplayItemLine[], explicit: [] as DisplayItemLine[],
  };
  lines.forEach((source, index) => {
    const line = { ...source, original: source.text, text: translated[index] ?? source.text, index };
    if (line.kind === 'name' && !result.name) { result.name = line.text; return; }
    if (line.kind === 'base') { result.bases.push(line); return; }
    if (line.kind === 'explicit') { result.explicit.push(line); return; }
    if (['implicit', 'enchant', 'rune', 'class_req'].includes(line.kind)) { result.effects.push(line); return; }
    // Separators and parser bookkeeping are replaced by sections; original text stays available.
    if (!line.original.trim() || /^[-—]+$/.test(line.original) || /^(Unique ID:|Implicits:|Requirements:\s*$)/i.test(line.original)) return;
    if (/^(Corrupted|Mirrored|Unidentified|Split|Sanctified)$/i.test(line.original)) { result.states.push(line); return; }
    if (/^(Rune|Soul Core):/i.test(line.original)) { result.runes.push(line); return; }
    const quality = /^Quality(?: \([^)]+ Modifiers\))?:/i.test(line.original);
    const property = quality || /^(Item Level|LevelReq|Requires Level|Str|Dex|Int|Armour|Evasion(?: Rating)?|Energy Shield|Ward|Physical Damage|Elemental Damage|Chaos Damage|Critical (?:Hit|Strike) Chance|Attacks per Second|Weapon Range|Block Chance|Charm Slots|Sockets|Radius|Limited to):/i.test(line.original);
    const parts = property && line.text.match(/^([^:：]+)[:：]\s*(.+)$/);
    if (parts) {
      const labels: Record<string, string> = { LevelReq: 'Required level', 'Requires Level': 'Required level', Str: 'Required Strength', Dex: 'Required Dexterity', Int: 'Required Intelligence' };
      let value = parts[2];
      if (quality && /^[+−-]?\d+(?:\.\d+)?$/.test(value)) value = `${Number(value) > 0 && !value.startsWith('+') ? '+' : ''}${value}%`;
      if (/^Sockets:/i.test(line.original) && /^(?:S\s*)+$/.test(value)) value = String(value.replace(/\s/g, '').length);
      result.properties.push({ label: labels[parts[1]] ?? parts[1], value, index });
    }
    else result.metadata.push(line);
  });
  return result;
}

// Item modifier tags use a different set of IDs from gem skill types.
const MOD_TAG_LABELS = new Map<string, readonly [string, string, string?]>([
  ['accuracy', ['Accuracy', '命中']],
  ['attack', ['Attack', '攻击', '攻擊']],
  ['attribute', ['Attribute', '属性', '屬性']],
  ['caster', ['Caster', '施法']],
  ['chaos', ['Chaos', '混沌']],
  ['cold', ['Cold', '冰霜', '冰冷']],
  ['critical', ['Critical', '暴击', '暴擊']],
  ['damage', ['Damage', '伤害', '傷害']],
  ['defences', ['Defences', '防御', '防禦']],
  ['elemental', ['Elemental', '元素']],
  ['fire', ['Fire', '火焰']],
  ['life', ['Life', '生命']],
  ['lightning', ['Lightning', '闪电', '閃電']],
  ['mana', ['Mana', '魔力']],
  ['minion', ['Minion', '召唤物', '召喚物']],
  ['physical', ['Physical', '物理']],
  ['resistance', ['Resistance', '抗性']],
  ['speed', ['Speed', '速度']],
]);

export function itemModTagLabel(tag: string, lang: Lang): string {
  const labels = MOD_TAG_LABELS.get(tag);
  if (!labels) return tag.replace(/[_-]+/g, ' ').replace(/^[a-z]/, first => first.toUpperCase());
  const [en, cn, tw = cn] = labels;
  return lang === 'en-US' ? en : lang === 'zh-TW' ? tw : cn;
}

/** Keep the sentence byte-for-byte while exposing signed values, ranges, decimals and percentages. */
export function itemValueParts(text: string): { text: string; numeric: boolean }[] {
  const parts: { text: string; numeric: boolean }[] = [];
  let offset = 0;
  for (const match of text.matchAll(/[+−-]?(?:\d{1,3}(?:,\d{3})+|\d+)(?:\.\d+)?%?/g)) {
    if (match.index > offset) parts.push({ text: text.slice(offset, match.index), numeric: false });
    parts.push({ text: match[0], numeric: true });
    offset = match.index + match[0].length;
  }
  if (offset < text.length) parts.push({ text: text.slice(offset), numeric: false });
  return parts;
}

/** Only an unambiguous damage type gets a semantic colour; mixed types remain neutral. */
export function itemDamageType(original: string): string | undefined {
  if (!/\b(?:Damage|Resistances?)\b/i.test(original)) return;
  const types = ['Physical', 'Fire', 'Cold', 'Lightning', 'Chaos'].filter(type => new RegExp(`\\b${type}\\b`, 'i').test(original));
  return types.length === 1 ? types[0].toLowerCase() : undefined;
}

export function itemAttributeType(original: string): string | undefined {
  const damage = itemDamageType(original);
  if (damage) return damage;
  const patterns: [string, RegExp][] = [
    ['life', /\b(?:Life|Life Regeneration)\b/i], ['mana', /\bMana\b/i],
    ['es', /\bEnergy Shield\b/i], ['defence', /\b(?:Armour|Evasion(?: Rating)?)\b/i],
  ];
  const attributes = patterns.filter(([, pattern]) => pattern.test(original));
  return attributes.length === 1 ? attributes[0][0] : undefined;
}
