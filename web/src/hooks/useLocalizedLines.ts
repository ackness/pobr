/**
 * 物品文本行的中文显示翻译（Items 面板 / 树页珠宝选择器共用）：
 * 结构标注行走本地前缀词典，其余行批量送 wasm en→zh 翻译层；翻不中原样显示。
 */

import { useEffect, useMemo, useState } from 'react';
import { getBackend } from '../api/backend';
import type { Lang } from '../lib/i18n';
import { lineText } from '../lib/continuedLines';
import type { GameNameRequest } from '../lib/nativeDisplayNames';

/** PoB 结构标注行的键 → 本地化（词条模板翻译层不认识这些行）。 */
const STRUCT_LINE_KEYS: Record<string, { 'zh-TW': string; 'zh-CN': string }> = {
  'Item Level': { 'zh-TW': '物品等級', 'zh-CN': '物品等级' },
  'LevelReq': { 'zh-TW': '需求等級', 'zh-CN': '需求等级' },
  'Requires Level': { 'zh-TW': '需求等級', 'zh-CN': '需求等级' },
  'Str': { 'zh-TW': '需求力量', 'zh-CN': '需求力量' },
  'Dex': { 'zh-TW': '需求敏捷', 'zh-CN': '需求敏捷' },
  'Int': { 'zh-TW': '需求智慧', 'zh-CN': '需求智慧' },
  'Implicits': { 'zh-TW': '固有詞綴', 'zh-CN': '固有词缀' },
  'Unique ID': { 'zh-TW': '唯一 ID', 'zh-CN': '唯一 ID' },
  'Quality': { 'zh-TW': '品質', 'zh-CN': '品质' },
  'Armour': { 'zh-TW': '護甲', 'zh-CN': '护甲' },
  'Evasion': { 'zh-TW': '閃避', 'zh-CN': '闪避' },
  'Evasion Rating': { 'zh-TW': '閃避值', 'zh-CN': '闪避值' },
  'Energy Shield': { 'zh-TW': '能量護盾', 'zh-CN': '能量护盾' },
  'Physical Damage': { 'zh-TW': '物理傷害', 'zh-CN': '物理伤害' },
  'Elemental Damage': { 'zh-TW': '元素傷害', 'zh-CN': '元素伤害' },
  'Chaos Damage': { 'zh-TW': '混沌傷害', 'zh-CN': '混沌伤害' },
  'Critical Hit Chance': { 'zh-TW': '暴擊機率', 'zh-CN': '暴击几率' },
  'Critical Strike Chance': { 'zh-TW': '暴擊機率', 'zh-CN': '暴击几率' },
  'Attacks per Second': { 'zh-TW': '每秒攻擊次數', 'zh-CN': '每秒攻击次数' },
  'Weapon Range': { 'zh-TW': '武器範圍', 'zh-CN': '武器范围' },
  'Block chance': { 'zh-TW': '格擋機率', 'zh-CN': '格挡几率' },
  'Block Chance': { 'zh-TW': '格擋機率', 'zh-CN': '格挡几率' },
  'Ward': { 'zh-TW': '結界', 'zh-CN': '结界' },
  'Charm Slots': { 'zh-TW': '護符插槽', 'zh-CN': '护符插槽' },
  'Radius': { 'zh-TW': '範圍', 'zh-CN': '范围' },
  'Limited to': { 'zh-TW': '限裝', 'zh-CN': '限装' },
  'Sockets': { 'zh-TW': '插槽', 'zh-CN': '插槽' },
};

const STRUCT_VALUES: Record<string, { 'zh-TW': string; 'zh-CN': string }> = {
  Small: { 'zh-TW': '小型', 'zh-CN': '小型' },
  Medium: { 'zh-TW': '中型', 'zh-CN': '中型' },
  Large: { 'zh-TW': '大型', 'zh-CN': '大型' },
  Corrupted: { 'zh-TW': '已汙染', 'zh-CN': '已腐化' },
  Mirrored: { 'zh-TW': '已複製', 'zh-CN': '已镜像' },
  Unidentified: { 'zh-TW': '未鑑定', 'zh-CN': '未鉴定' },
  Split: { 'zh-TW': '已分割', 'zh-CN': '已分割' },
};

/** 符文/魂核命名行前缀（值是物品名，需单独送翻译）。 */
const RUNE_PREFIXES: Record<string, { 'zh-TW': string; 'zh-CN': string }> = {
  'Rune': { 'zh-TW': '符文', 'zh-CN': '符文' },
  'Soul Core': { 'zh-TW': '魂核', 'zh-CN': '魂核' },
};

function runeLineParts(line: string): { prefix: string; name: string } | null {
  const m = line.match(/^(Rune|Soul Core):\s*(.+)$/);
  return m ? { prefix: m[1], name: m[2].trim() } : null;
}

/** 从 PoB 原始文本块提取稀有度（`Rarity: RARE` 行；XML 导入的该行带缩进）。 */
export function rarityOf(text: string): string {
  const m = text.match(/^\s*Rarity:\s*(\w+)/im);
  return (m?.[1] ?? 'normal').toLowerCase();
}

/** 展示用：剥掉行内 `{tags}` 与 `[A|B]` 标注（PoB 词条内部标注语法）。 */
export function cleanLine(line: string): string {
  return line
    .replace(/\{[^}]*\}/g, '')
    .replace(/\[([^\]|]*)\|([^\]]*)\]/g, '$2')
    .replace(/\[([^\]]*)\]/g, '$1')
    .trim();
}

/** 物品文本 → 展示行（剥标注、滤 Rarity 行）。 */
export function itemLines(text: string): string[] {
  return text
    .split('\n')
    .map(cleanLine)
    .filter((l) => l && !/^Rarity:/i.test(l));
}

/** 展示名行对：RARE/UNIQUE 物品 = [随机名, 基底]（玩家光看随机名不知道是什么装备）；
 * MAGIC/NORMAL 名称行本身含基底，只取首行。 */
export function nameParts(text: string): string[] {
  const lines = itemLines(text);
  const rarity = rarityOf(text);
  if ((rarity === 'rare' || rarity === 'unique') && lines[1]) {
    return [lines[0], lines[1]];
  }
  return lines[0] ? [lines[0]] : [];
}

/** 批量物品显示名「名称·基底」（已本地化；空文本 → 空串）。
 * 库行/槽位按钮/切换下拉等所有名称展示点共用。 */
export function useItemDisplayNames(texts: (string | undefined)[], lang: Lang): string[] {
  const parts = texts.map((t) => (t ? nameParts(t) : []));
  // The second part of a rare/unique pair is the item format's base line, resolved by base identity.
  const translated = useLocalizedLines(parts.flatMap((p) => p.slice(0, 1)), lang);
  const bases = useLocalizedNames(parts.flatMap((p) => p.slice(1).map((name) => ({ kind: 'base' as const, name }))), lang);
  let name = 0, base = 0;
  return parts.map((p) => p.map((_, index) => index === 0 ? translated[name++] : bases[base++]).join('·'));
}

/** 结构行本地化：`Item Level: 83` → `物品等级: 83`；非结构行返回 null。 */
export function localizeStructLine(line: string, lang: Lang): string | null {
  if (lang === 'en-US') return null;
  if (STRUCT_VALUES[line]) return STRUCT_VALUES[line][lang];
  const m = line.match(/^([A-Za-z' ]+):\s*(.*)$/);
  if (!m) return null;
  const entry = STRUCT_LINE_KEYS[m[1].trim()];
  const value = STRUCT_VALUES[m[2]]?.[lang] ?? m[2];
  return entry ? `${entry[lang]}: ${value}` : null;
}

/** A rune's `Bonded: <effect>` line. The language pack owns its whole translation (official
 * whole-line rows, or a `Bonded: {0}` template whose value is the effect's own translation). */
const BONDED = /^Bonded:\s*/;
/** Sent once with a batch that has a `Bonded:` line: the pack's own rendering of the prefix. */
const BONDED_PREFIX_PROBE = 'Bonded: {0}';

/** What {@link useLocalizedLines} sends `translateLines` for `lines`: one entry per line, in order.
 * 符文命名行只送名称部分（"Rune: X" 整行不在词条模板里，X 是基底名可直译）。A blank line, or one
 * translated locally, is sent blank: it still separates its neighbours, so lines that were not
 * adjacent never translate as one description. A `Bonded:` line is sent whole; after the lines,
 * each one's effect and {@link BONDED_PREFIX_PROBE} follow (each after a blank), for a line the
 * pack has no whole-line translation of. */
export function linesToTranslate(lines: readonly string[], lang: Lang): string[] {
  const sent = lines.map((l) => (!l || localizeStructLine(l, lang) ? '' : runeLineParts(l)?.name ?? l));
  const bonded = lines.filter((l, index) => sent[index] && BONDED.test(l));
  if (!bonded.length) return sent;
  return [...sent, ...bonded.flatMap((l) => ['', l.replace(BONDED, '')]), '', BONDED_PREFIX_PROBE];
}

/** The pack's display of `Bonded: <effect>` when it has no whole-line translation: its rendering of
 * the prefix around the effect's own translation; English when either is missing. */
function composedBonded(line: string, effect: string | undefined, prefix: string | undefined): string {
  const bare = line.replace(BONDED, '');
  return effect && effect !== bare && prefix && prefix !== BONDED_PREFIX_PROBE && prefix.includes('{0}')
    ? prefix.replace('{0}', effect) : line;
}

/** The display of `lines` given `translated`, the backend's answer to {@link linesToTranslate}
 * matched by position (not by text: one line can be covered by a multi-line description in one
 * place and stand alone in another). A covered line stays `""` (see `lib/continuedLines`). In
 * en-US the result is `lines` itself, so it stays the same array while `lines` does. */
export function localizedLines(lines: string[], lang: Lang, translated: readonly (string | undefined)[]): string[] {
  if (lang === 'en-US') return lines;
  let extra = lines.length;
  const prefix = translated.at(-1);
  return lines.map((l, index) => {
    const struct = localizeStructLine(l, lang);
    if (struct) return struct;
    const out = translated[index];
    const rune = runeLineParts(l);
    if (rune) {
      return `${RUNE_PREFIXES[rune.prefix][lang]}: ${lineText(rune.name, out)}`;
    }
    if (BONDED.test(l)) {
      extra += 2;
      if (out !== undefined && out !== l && !BONDED.test(out)) return out;
      // No whole-line row, or one that kept the English prefix: compose from the pack's own prefix.
      const composed = composedBonded(l, translated[extra - 1], prefix);
      return composed === l && out !== undefined ? out : composed;
    }
    return out ?? l;
  });
}

/** No answer yet: one shared array, so the memo below keeps its value across renders. */
const NO_TRANSLATIONS: readonly (string | undefined)[] = [];

/** 中文界面：词条行批量反查翻译（结构行走本地词典；翻译不中原样显示）。One output per input; a line a
 * multi-line description covered comes back `""`, which consumers drop or replace through
 * `lib/continuedLines`. */
export function useLocalizedLines(lines: string[], lang: Lang): string[] {
  const key = JSON.stringify(lines);
  const [result, setResult] = useState<{ key: string; lang: Lang; values: (string | undefined)[] } | null>(null);
  const translated = result?.key === key && result.lang === lang ? result.values : NO_TRANSLATIONS;
  useEffect(() => {
    if (lang === 'en-US' || lines.length === 0) return;
    const sent = linesToTranslate(lines, lang);
    if (!sent.some(Boolean)) return;
    let cancelled = false;
    getBackend()
      .then((b) => b.translateLines(sent, lang))
      .then((out) => {
        if (cancelled) return;
        setResult({ key, lang, values: sent.map((text, index) => (text ? out[index] : undefined)) });
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, lang]);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  return useMemo(() => localizedLines(lines, lang, translated), [key, lang, translated]);
}

/** Names whose identity is known (passive node id, item base line) → display, batched like
 * `useLocalizedLines`. Until the backend answers, and on failure, the English name is shown. */
export function useLocalizedNames(names: readonly GameNameRequest[], lang: Lang): string[] {
  const key = useMemo(() => JSON.stringify(names), [names]);
  const [result, setResult] = useState<{ key: string; lang: Lang; values: string[] } | null>(null);
  useEffect(() => {
    if (lang === 'en-US' || names.length === 0) return;
    let cancelled = false;
    getBackend()
      .then((b) => b.translateNames(names, lang))
      .then((values) => { if (!cancelled) setResult({ key, lang, values }); })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, lang]);
  return useMemo(() => {
    const values = lang !== 'en-US' && result?.key === key && result.lang === lang ? result.values : [];
    return names.map((name, index) => values[index] ?? name.name);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, lang, result]);
}
