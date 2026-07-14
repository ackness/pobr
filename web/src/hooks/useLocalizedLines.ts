/**
 * 物品文本行的中文显示翻译（Items 面板 / 树页珠宝选择器共用）：
 * 结构标注行走本地前缀词典，其余行批量送 wasm en→zh 翻译层；翻不中原样显示。
 */

import { useEffect, useMemo, useState } from 'react';
import { getBackend } from '../api/backend';
import type { Lang } from '../lib/i18n';

/** PoB 结构标注行的键 → 本地化（词条模板翻译层不认识这些行）。 */
const STRUCT_LINE_KEYS: Record<string, { 'zh-TW': string; 'zh-CN': string }> = {
  'Item Level': { 'zh-TW': '物品等級', 'zh-CN': '物品等级' },
  'LevelReq': { 'zh-TW': '需求等級', 'zh-CN': '需求等级' },
  'Requires Level': { 'zh-TW': '需求等級', 'zh-CN': '需求等级' },
  'Implicits': { 'zh-TW': '固有詞綴', 'zh-CN': '固有词缀' },
  'Unique ID': { 'zh-TW': '唯一 ID', 'zh-CN': '唯一 ID' },
  'Quality': { 'zh-TW': '品質', 'zh-CN': '品质' },
  'Armour': { 'zh-TW': '護甲', 'zh-CN': '护甲' },
  'Evasion': { 'zh-TW': '閃避', 'zh-CN': '闪避' },
  'Energy Shield': { 'zh-TW': '能量護盾', 'zh-CN': '能量护盾' },
  'Ward': { 'zh-TW': '結界', 'zh-CN': '结界' },
  'Charm Slots': { 'zh-TW': '護符插槽', 'zh-CN': '护符插槽' },
  'Radius': { 'zh-TW': '範圍', 'zh-CN': '范围' },
  'Limited to': { 'zh-TW': '限裝', 'zh-CN': '限装' },
  'Sockets': { 'zh-TW': '插槽', 'zh-CN': '插槽' },
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
  const translated = useLocalizedLines(
    parts.flatMap((p) => p),
    lang,
  );
  let offset = 0;
  return parts.map((p) => {
    const segment = translated.slice(offset, offset + p.length);
    offset += p.length;
    return segment.join('·');
  });
}

/** 结构行本地化：`Item Level: 83` → `物品等级: 83`；非结构行返回 null。 */
export function localizeStructLine(line: string, lang: Lang): string | null {
  if (lang === 'en-US') return null;
  const m = line.match(/^([A-Za-z' ]+):\s*(.*)$/);
  if (!m) return null;
  const entry = STRUCT_LINE_KEYS[m[1].trim()];
  return entry ? `${entry[lang]}: ${m[2]}` : null;
}

/** 中文界面：词条行批量反查翻译（结构行走本地词典；翻译不中原样显示）。 */
export function useLocalizedLines(lines: string[], lang: Lang): string[] {
  const key = lines.join('\n');
  const [translated, setTranslated] = useState<Record<string, string>>({});
  useEffect(() => {
    if (lang === 'en-US' || lines.length === 0) return;
    // 符文命名行只送名称部分（"Rune: X" 整行不在词条模板里，X 是基底名可直译）。
    const pending = lines
      .filter((l) => l && !localizeStructLine(l, lang))
      .map((l) => runeLineParts(l)?.name ?? l);
    if (pending.length === 0) return;
    let cancelled = false;
    getBackend()
      .then((b) => b.translateLines(pending))
      .then((out) => {
        if (cancelled) return;
        const map: Record<string, string> = {};
        pending.forEach((en, i) => {
          if (out[i] !== en) map[en] = out[i];
        });
        setTranslated(map);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, lang]);
  return useMemo(
    () =>
      lang === 'en-US'
        ? lines
        : lines.map((l) => {
            const struct = localizeStructLine(l, lang);
            if (struct) return struct;
            const rune = runeLineParts(l);
            if (rune) {
              return `${RUNE_PREFIXES[rune.prefix][lang]}: ${translated[rune.name] ?? rune.name}`;
            }
            return translated[l] ?? l;
          }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [key, lang, translated],
  );
}
