import type { StatComparison } from '../../../lib/compare';
import type { Lang } from '../../../lib/i18n';
import { words } from '../../text';
import { statPolarity } from '../../motion/statPolarity';

/** The five parts of a stage comparison, in the order a reader should visit them. */
export const COMPARE_TABS = ['stats', 'items', 'skills', 'config', 'tree'] as const;
export type CompareTab = typeof COMPARE_TABS[number];
export type DiffCounts = Record<CompareTab, number>;
export type DeltaTone = 'better' | 'worse' | 'neutral';

/** Stats that already carry the answer the reader came for; shown beside the summary. */
export const HEADLINE_STATS = ['CombinedDPS', 'TotalDPS', 'TotalEHP', 'Life'] as const;

export function totalChanges(counts: DiffCounts): number {
  return COMPARE_TABS.reduce((sum, tab) => sum + counts[tab], 0);
}

/** Open on the first part that has something to show, so the page never starts on an empty tab. */
export function defaultCompareTab(counts: DiffCounts): CompareTab {
  return COMPARE_TABS.find(tab => counts[tab] > 0) ?? 'stats';
}

/** A larger value is not always an improvement (costs, requirements); unknown direction stays neutral. */
export function deltaTone(id: string, delta: number | null): DeltaTone {
  if (delta === null || delta === 0) return 'neutral';
  const polarity = statPolarity(id);
  return polarity === 0 ? 'neutral' : delta * polarity > 0 ? 'better' : 'worse';
}

const NOUNS: Record<CompareTab, [string, string, string, string]> = {
  stats: ['stat', 'stats', '项属性', '項屬性'],
  items: ['item', 'items', '件装备', '件裝備'],
  skills: ['skill group', 'skill groups', '个技能组', '個技能組'],
  config: ['setting', 'settings', '项配置', '項配置'],
  tree: ['passive node', 'passive nodes', '个天赋节点', '個天賦節點'],
};

/** "3 stats", "1 skill group", "3 项属性". Chinese counts need no plural. */
export function countPhrase(lang: Lang, tab: CompareTab, count: number): string {
  const [one, many, cn, tw] = NOUNS[tab];
  return lang === 'en-US' ? `${count} ${count === 1 ? one : many}` : `${count} ${lang === 'zh-TW' ? tw : cn}`;
}

/** One line: what B changes relative to A. Parts with no change are left out. */
export function summaryLine(lang: Lang, counts: DiffCounts): string {
  const parts = COMPARE_TABS.filter(tab => counts[tab] > 0).map(tab => countPhrase(lang, tab, counts[tab]));
  if (!parts.length) return words(lang, 'A and B are identical.', 'A 与 B 完全相同。', 'A 與 B 完全相同。');
  const list = lang === 'en-US' ? (parts.length > 1 ? `${parts.slice(0, -1).join(', ')} and ${parts.at(-1)}` : parts[0]) : parts.join('、');
  return words(lang, `B changes ${list}.`, `B 相对 A 有 ${list}不同。`, `B 相對 A 有 ${list}不同。`);
}

export interface Headline { id: string; percent: number; tone: DeltaTone }

/** Up to two headline movements (damage first, then survivability) as signed percentages of A. */
export function headlineDeltas(rows: StatComparison[]): Headline[] {
  const find = (ids: readonly string[]) => ids.map(id => rows.find(row => row.id === id))
    .find(row => row && row.changed && row.delta !== null && row.percent !== null);
  return [find(['CombinedDPS', 'TotalDPS']), find(['TotalEHP']), find(['Life'])]
    .flatMap(row => row ? [{ id: row.id, percent: row.percent!, tone: deltaTone(row.id, row.delta) }] : []).slice(0, 2);
}

/** Passive allocation differences. Numeric ids only mean the same node inside one tree version. */
export function treeDifferenceCount(
  a: { allocatedNodes: number[]; jewels: { socket_node: number; text: string }[] },
  b: { allocatedNodes: number[]; jewels: { socket_node: number; text: string }[] },
  sameVersion: boolean,
): number | null {
  const left = new Set(a.allocatedNodes), right = new Set(b.allocatedNodes);
  const jewels = (state: typeof a) => new Map(state.jewels.map(jewel => [jewel.socket_node, jewel.text]));
  const ja = jewels(a), jb = jewels(b);
  const changedJewels = [...new Set([...ja.keys(), ...jb.keys()])].filter(id => ja.get(id) !== jb.get(id)).length;
  if (!sameVersion) return changedJewels === 0 && left.size === 0 && right.size === 0 ? 0 : null;
  const moved = [...left].filter(id => !right.has(id)).length + [...right].filter(id => !left.has(id)).length;
  return moved + changedJewels;
}

export function signedPercent(percent: number, digits = 1): string {
  const text = Math.abs(percent).toFixed(digits);
  return `${percent > 0 ? '+' : percent < 0 ? '−' : ''}${text}%`;
}

export interface DiffLine { text: string; changed: boolean }
export type DiffRow = { kind: 'line'; text: string; changed: boolean } | { kind: 'gap'; count: number };

/**
 * Show what changed with a little context: blank lines go, runs of unchanged lines collapse into one gap.
 * Leading item-text indentation is display noise, so it is trimmed here and only here.
 */
export function collapseContext(lines: DiffLine[], context = 1): DiffRow[] {
  const meaningful = lines.filter(line => line.text.trim());
  const keep = meaningful.map((line, index) => line.changed
    || meaningful.slice(Math.max(0, index - context), index + context + 1).some(near => near.changed));
  const rows: DiffRow[] = [];
  let skipped = 0;
  meaningful.forEach((line, index) => {
    if (keep[index]) {
      if (skipped) rows.push({ kind: 'gap', count: skipped });
      skipped = 0;
      rows.push({ kind: 'line', text: line.text.trimStart(), changed: line.changed });
    } else skipped++;
  });
  if (skipped) rows.push({ kind: 'gap', count: skipped });
  return rows;
}
