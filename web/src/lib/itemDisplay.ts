import type { ItemLineJson } from '../api/types';
export { DISPLAY_MODES as ITEM_DISPLAY_STYLES } from './displayModes';
export type { DisplayMode as ItemDisplayStyle } from './displayModes';

export interface DisplayItemLine extends ItemLineJson { original: string; index: number }
export interface ItemProperty { label: string; value: string; index: number }

/** Presentation only: never infer an affix's identity or capacity from its position or line count. */
export function itemDisplaySections(lines: readonly ItemLineJson[], translated: readonly string[]) {
  const result = {
    name: '', bases: [] as DisplayItemLine[], states: [] as DisplayItemLine[],
    properties: [] as ItemProperty[], metadata: [] as DisplayItemLine[], runes: [] as DisplayItemLine[],
    effects: [] as DisplayItemLine[], prefixes: [] as DisplayItemLine[], suffixes: [] as DisplayItemLine[],
    other: [] as DisplayItemLine[], explicit: [] as DisplayItemLine[],
  };
  lines.forEach((source, index) => {
    const line = { ...source, original: source.text, text: translated[index] ?? source.text, index };
    if (line.kind === 'name' && !result.name) { result.name = line.text; return; }
    if (line.kind === 'base') { result.bases.push(line); return; }
    if (line.kind === 'explicit') {
      result.explicit.push(line);
      if (line.affix === 'prefix') result.prefixes.push(line);
      else if (line.affix === 'suffix') result.suffixes.push(line);
      else result.other.push(line);
      return;
    }
    if (['implicit', 'enchant', 'rune', 'class_req'].includes(line.kind)) { result.effects.push(line); return; }
    // Separators and parser bookkeeping are replaced by sections; original text stays available.
    if (!line.original.trim() || /^[-—]+$/.test(line.original) || /^(Unique ID:|Implicits:|Requirements:\s*$)/i.test(line.original)) return;
    if (/^(Corrupted|Mirrored|Unidentified|Split|Sanctified)$/i.test(line.original)) { result.states.push(line); return; }
    if (/^(Rune|Soul Core):/i.test(line.original)) { result.runes.push(line); return; }
    const property = /^(Item Level|Quality|LevelReq|Requires Level|Str|Dex|Int|Armour|Evasion|Energy Shield|Ward|Physical Damage|Elemental Damage|Chaos Damage|Critical (?:Hit|Strike) Chance|Attacks per Second|Weapon Range|Block Chance|Charm Slots|Sockets|Radius|Limited to):/i.test(line.original);
    const parts = property && line.text.match(/^([^:：]+)[:：]\s*(.+)$/);
    if (parts) {
      const labels: Record<string, string> = { LevelReq: 'Required level', 'Requires Level': 'Required level', Str: 'Required Strength', Dex: 'Required Dexterity', Int: 'Required Intelligence' };
      let value = parts[2];
      if (/^Quality:/i.test(line.original) && /^[+−-]?\d+(?:\.\d+)?$/.test(value)) value = `${Number(value) > 0 && !value.startsWith('+') ? '+' : ''}${value}%`;
      if (/^Sockets:/i.test(line.original) && /^(?:S\s*)+$/.test(value)) value = String(value.replace(/\s/g, '').length);
      result.properties.push({ label: labels[parts[1]] ?? parts[1], value, index });
    }
    else result.metadata.push(line);
  });
  return result;
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
  const types = ['Physical', 'Fire', 'Cold', 'Lightning', 'Chaos'].filter(type => new RegExp(`\\b${type} Damage\\b`, 'i').test(original));
  return types.length === 1 ? types[0].toLowerCase() : undefined;
}
