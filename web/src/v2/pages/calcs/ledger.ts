import type { Lang } from '../../../lib/i18n';
import type { LineUsage, LineUsageLine, LineUsageStatus } from '../../../api/types';
import { words } from '../../text';

/**
 * The ledger answers "where did every line of this BD go?". The engine already gives each
 * line a fate; this module only orders, filters and counts them so the page stays a thin view.
 */

/** Lines that need a person's attention come first; confirmed-good and known-harmless lines last. */
export const STATUS_ORDER: readonly LineUsageStatus[] = [
  'unparsed', 'unmodelled', 'inactive', 'unread', 'no_modifier', 'partial', 'outside', 'no_effect', 'applied',
];

export type StatusTone = 'bad' | 'warn' | 'ok' | 'neutral';

/** bad: the line is lost. warn: it is understood but does nothing right now. ok: counted. neutral: nothing to do. */
export function statusTone(status: LineUsageStatus): StatusTone {
  switch (status) {
    case 'unparsed': case 'unmodelled': case 'no_modifier': return 'bad';
    case 'inactive': case 'unread': case 'partial': return 'warn';
    case 'applied': return 'ok';
    case 'outside': case 'no_effect': return 'neutral';
  }
}

/** Statuses where the BD's author may want to act (turn on a config option, report a gap). */
export const ATTENTION: ReadonlySet<LineUsageStatus> = new Set(['unparsed', 'unmodelled', 'inactive', 'unread', 'no_modifier']);

const RANK = new Map(STATUS_ORDER.map((status, index) => [status, index]));
export function statusRank(status: LineUsageStatus): number { return RANK.get(status) ?? STATUS_ORDER.length; }

export function statusLabel(lang: Lang, status: LineUsageStatus): string {
  const w = (en: string, cn: string, tw = cn) => words(lang, en, cn, tw);
  switch (status) {
    case 'applied': return w('Applied', '已生效', '已生效');
    case 'partial': return w('Partly applied', '部分生效', '部分生效');
    case 'inactive': return w('Inactive', '未触发', '未觸發');
    case 'unread': return w('Not read', '无人读取', '無人讀取');
    case 'outside': return w('Used elsewhere', '其他途径使用', '其他途徑使用');
    case 'no_effect': return w('No effect', '无计算影响', '無計算影響');
    case 'unmodelled': return w('Not modelled', '尚未建模', '尚未建模');
    case 'unparsed': return w('Not understood', '无法解析', '無法解析');
    case 'no_modifier': return w('Not stored', '未存入', '未存入');
  }
}

export function statusHint(lang: Lang, status: LineUsageStatus): string {
  const w = (en: string, cn: string, tw = cn) => words(lang, en, cn, tw);
  switch (status) {
    case 'applied': return w('Counted in the calculation.', '已计入计算。', '已計入計算。');
    case 'partial': return w('Some parts of the line are counted and others are not.', '这一行的部分内容已计入，其余没有。', '這一行的部分內容已計入，其餘沒有。');
    case 'inactive': return w('The calculation looks for it, but a condition or skill requirement is not met right now.', '计算会读取它，但当前条件或技能要求不满足。', '計算會讀取它，但目前條件或技能要求不滿足。');
    case 'unread': return w('Understood correctly, but nothing in the calculation asks for it.', '已正确解析，但计算中没有任何地方读取它。', '已正確解析，但計算中沒有任何地方讀取它。');
    case 'outside': return w('Used by a step outside the modifier store: item defences, gem levels, jewel radius.', '由词条库之外的环节使用：装备自身防御、宝石等级、珠宝范围等。', '由詞綴庫之外的環節使用：裝備自身防禦、寶石等級、珠寶範圍等。');
    case 'no_effect': return w('Recognised and known not to change any output.', '已识别，且确认不影响任何输出。', '已識別，且確認不影響任何輸出。');
    case 'unmodelled': return w('Recognised, but the mechanic is not implemented yet.', '已识别，但该机制尚未实现。', '已識別，但該機制尚未實現。');
    case 'unparsed': return w('No rule could read this line, so it has no effect.', '没有规则能解析这一行，因此它没有任何效果。', '沒有規則能解析這一行，因此它沒有任何效果。');
    case 'no_modifier': return w('Read into a modifier, but it was not stored for the calculation.', '已解析出词条，但没有存入计算。', '已解析出詞綴，但沒有存入計算。');
  }
}

export type SourceGroup = 'item' | 'tree' | 'jewel' | 'flask' | 'config' | 'skill' | 'other';
export const SOURCE_GROUPS: readonly SourceGroup[] = ['item', 'tree', 'jewel', 'flask', 'config', 'skill', 'other'];

/** Where a line comes from, by its source id (`item.helmet.explicit`, `node.123`, `flask.flask1`, `quest`…). */
export function sourceGroup(line: Pick<LineUsageLine, 'source_id' | 'source_kind'>): SourceGroup {
  const id = (line.source_id ?? '').toLowerCase();
  const kind = (line.source_kind ?? '').toLowerCase();
  if (id.startsWith('item.') || id.startsWith('limb.') || kind.startsWith('item') || kind === 'limb') return 'item';
  if (id.startsWith('node.') || kind === 'passivenode') return 'tree';
  if (id.startsWith('jewel.') || kind === 'jewel') return 'jewel';
  if (id.startsWith('flask.') || id.startsWith('charm.') || kind === 'flask' || kind === 'charm') return 'flask';
  if (id === 'quest' || id.startsWith('config.') || id.startsWith('quest') || kind === 'config') return 'config';
  if (id.startsWith('gem.') || id.startsWith('skill.') || kind === 'gem' || kind === 'skill') return 'skill';
  return 'other';
}

export function sourceGroupLabel(lang: Lang, group: SourceGroup): string {
  const w = (en: string, cn: string, tw = cn) => words(lang, en, cn, tw);
  switch (group) {
    case 'item': return w('Equipment', '装备', '裝備');
    case 'tree': return w('Passive tree', '天赋树', '天賦樹');
    case 'jewel': return w('Jewels', '珠宝', '珠寶');
    case 'flask': return w('Flasks & charms', '药剂与护符', '藥劑與護符');
    case 'config': return w('Config & rewards', '配置与奖励', '配置與獎勵');
    case 'skill': return w('Skills', '技能', '技能');
    case 'other': return w('Other', '其他', '其他');
  }
}

/** Strip `[A|B]` / `[A]` display markup from game text. */
export function cleanLineText(text: string): string {
  return text.replace(/\[([^\]|]*)\|([^\]]*)\]/g, '$2').replace(/\[([^\]]*)\]/g, '$1');
}

/** The text a row shows: the original line, or (for gem stats that have no line) the first modifier's name. */
export function lineText(line: LineUsageLine): string {
  if (line.text) return cleanLineText(line.text);
  return line.modifiers[0]?.name ?? '—';
}

export interface LedgerFilter {
  /** Show only these statuses; null shows all. */
  statuses: ReadonlySet<LineUsageStatus> | null;
  query: string;
  group: SourceGroup | 'all';
}

export const NO_FILTER: LedgerFilter = { statuses: null, query: '', group: 'all' };

function searchable(line: LineUsageLine): string {
  return [
    lineText(line), line.holder, line.reason, line.source_id, line.slot, line.source_kind, ...line.used_in,
    ...line.modifiers.flatMap(mod => [mod.name, mod.reason]),
  ].filter(Boolean).join('\n').toLowerCase();
}

export function matchesFilter(line: LineUsageLine, filter: LedgerFilter): boolean {
  if (filter.statuses && !filter.statuses.has(line.status)) return false;
  if (filter.group !== 'all' && sourceGroup(line) !== filter.group) return false;
  const query = filter.query.trim().toLowerCase();
  return query === '' || searchable(line).includes(query);
}

/** Attention-first order. Within a status, the engine's own order (which follows the build) is kept. */
export function sortLedger(lines: readonly LineUsageLine[]): LineUsageLine[] {
  return lines.map((line, index) => ({ line, index }))
    .sort((a, b) => statusRank(a.line.status) - statusRank(b.line.status) || a.index - b.index)
    .map(entry => entry.line);
}

export function filterLedger(lines: readonly LineUsageLine[], filter: LedgerFilter): LineUsageLine[] {
  return sortLedger(lines.filter(line => matchesFilter(line, filter)));
}

export interface LedgerSummary {
  total: number;
  counts: Record<LineUsageStatus, number>;
  /** Lines in a state the author may want to act on. */
  attention: number;
  /** Share of lines that are applied, 0–100 (whole lines only). */
  appliedPercent: number;
  groups: Record<SourceGroup, number>;
}

export function summarise(usage: Pick<LineUsage, 'lines'>): LedgerSummary {
  const counts = Object.fromEntries(STATUS_ORDER.map(status => [status, 0])) as Record<LineUsageStatus, number>;
  const groups = Object.fromEntries(SOURCE_GROUPS.map(group => [group, 0])) as Record<SourceGroup, number>;
  for (const line of usage.lines) {
    counts[line.status] = (counts[line.status] ?? 0) + 1;
    groups[sourceGroup(line)] += 1;
  }
  const total = usage.lines.length;
  const attention = STATUS_ORDER.filter(status => ATTENTION.has(status)).reduce((sum, status) => sum + counts[status], 0);
  return { total, counts, attention, appliedPercent: total === 0 ? 0 : Math.round(counts.applied / total * 100), groups };
}

/** Toggle one status in a filter set; an emptied set means "show all" again. */
export function toggleStatus(current: ReadonlySet<LineUsageStatus> | null, status: LineUsageStatus): ReadonlySet<LineUsageStatus> | null {
  const next = new Set(current ?? []);
  if (next.has(status)) next.delete(status); else next.add(status);
  return next.size === 0 ? null : next;
}
