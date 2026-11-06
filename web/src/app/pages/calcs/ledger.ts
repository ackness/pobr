import { plainModifierReason } from '../../../lib/diagnosticText';
import { cleanLineText } from '../../../lib/modifierText';
import { originLabel, type Lang } from '../../../lib/i18n';
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
    case 'inactive': return w('Inactive', '当前不生效', '目前不生效');
    case 'unread': return w('Unused', '未被使用', '未被使用');
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
    case 'outside': return w('Used by a step outside the modifier store: item defences, gem levels, jewel radius.', '由词缀库之外的环节使用：装备自身防御、宝石等级、珠宝范围等。', '由詞綴庫之外的環節使用：裝備自身防禦、寶石等級、珠寶範圍等。');
    case 'no_effect': return w('Recognised and known not to change any output.', '已识别，且确认不影响任何输出。', '已識別，且確認不影響任何輸出。');
    case 'unmodelled': return w('Recognised, but the mechanic is not implemented yet.', '已识别，但该机制尚未实现。', '已識別，但該機制尚未實現。');
    case 'unparsed': return w('No rule could read this line, so this calculation does not include its effect.', '没有规则能解析这一行，本次计算未计入其效果。', '沒有規則能解析這一行，本次計算未計入其效果。');
    case 'no_modifier': return w('Read into a modifier, but it was not stored for the calculation.', '已解析出词缀，但没有存入计算。', '已解析出詞綴，但沒有存入計算。');
  }
}

export type SourceGroup = 'item' | 'tree' | 'jewel' | 'flask' | 'config' | 'skill' | 'other';
export const SOURCE_GROUPS: readonly SourceGroup[] = ['item', 'tree', 'jewel', 'flask', 'config', 'skill', 'other'];

/** Where a line comes from, by its source id (`item.helmet.explicit`, `node.123`, `flask.flask1`, `quest`…). */
export function sourceGroup(line: Pick<LineUsageLine, 'source_id' | 'source_kind'>): SourceGroup {
  const id = (line.source_id ?? '').toLowerCase();
  const kind = (line.source_kind ?? '').toLowerCase();
  if (id.startsWith('item.') || id.startsWith('limb.') || kind.startsWith('item') || kind === 'limb') return 'item';
  if (id.startsWith('node.') || ['passivenode', 'ascendancynode', 'grantedkeystone'].includes(kind)) return 'tree';
  if (id.startsWith('jewel.') || kind === 'jewel') return 'jewel';
  if (id.startsWith('flask.') || id.startsWith('charm.') || kind === 'flask' || kind === 'charm') return 'flask';
  if (id === 'quest' || id.startsWith('config.') || id.startsWith('quest') || ['config', 'configoption', 'enemyconfig', 'campaignreward'].includes(kind)) return 'config';
  if (id.startsWith('gem.') || id.startsWith('skill.') || ['gem', 'skill', 'skillgem', 'supportgem', 'skilllevel', 'gemquality'].includes(kind)) return 'skill';
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

/** The text a row shows: the original line, or (for gem stats that have no line) the first modifier's name. */
export function lineText(line: LineUsageLine): string {
  if (line.text) return cleanLineText(line.text);
  return line.modifiers[0]?.name ?? '—';
}

/** Engine-generated descriptions are UI text, not imported game-text templates. */
export function ledgerDisplayText(line: LineUsageLine, lang: Lang, configLabels: Readonly<Record<string, string>> = {}): string {
  const text = lineText(line);
  if (line.source_id?.startsWith('config.')) {
    const knownLabel = configLabels[line.source_id.slice(7)];
    // A known option's bare modifier id is a generated placeholder, not game text.
    if (text === 'Config' || (knownLabel && line.modifiers.some(mod => mod.name === text))) {
      const resolved = knownLabel ?? originLabel(lang, line.source_id);
      const label = resolved === line.source_id ? words(lang, 'Setting name unavailable', '缺少配置名称', '缺少設定名稱') : resolved;
      return words(lang, `Configuration: ${label}`, `配置：${label}`, `配置：${label}`);
    }
  }
  if (line.source_id === 'base.AncestralBoost'
    && text === 'Ancestral Boost: 2 additional Strike targets; 30% more Slam damage and 25% increased Slam area') {
    return words(lang, text,
      '先祖增幅：打击额外以 2 个敌人为目标；猛击伤害总增 30%，范围扩大 25%',
      '先祖增幅：打擊額外以 2 個敵人為目標；重擊傷害總增 30%，範圍增加 25%');
  }
  return text;
}

/** Explain an inactive row without making the user expand its modifier table. */
export function ledgerReason(line: LineUsageLine, lang: Lang): string | null {
  const reasons = line.reason ? [line.reason] : line.modifiers
    .filter(mod => mod.status === 'inactive' || mod.status === 'unread')
    .flatMap(mod => mod.reason ? [mod.reason] : []);
  const localized = [...new Set(reasons)].map(reason => {
    if (lang !== 'en-US') {
      if (reason === 'its value is 0') {
        return words(lang, reason, '数值为 0。', '數值為 0。');
      }
      if (/^SkillType \w+(?: · SkillType \w+)*$/.test(reason)) {
        return words(lang, reason, '当前技能类型不满足词缀要求。', '目前技能類型不滿足詞綴要求。');
      }
      if (line.status === 'unmodelled' && /^rule \S+ recognises it and stores nothing$/.test(reason)) {
        return statusHint(lang, 'unmodelled');
      }
      if (reason === 'a config option nothing in this build asks for') {
        return words(lang, reason, '当前 BD 没有使用此配置项。', '目前 BD 沒有使用此配置項。');
      }
      const condition = /^Condition (Combat|Buffs|Effective|AncestralBoost|EnemyPoisoned) not met$/.exec(reason);
      if (condition) {
        const labels: Record<string, string> = {
          Combat: words(lang, 'Combat', '战斗状态', '戰鬥狀態'),
          Buffs: words(lang, 'Buffs', '增益效果', '增益效果'),
          Effective: words(lang, 'Effective', '有效配置', '有效設定'),
          AncestralBoost: words(lang, 'Ancestral Boost', '先祖增幅', '先祖增幅'),
          EnemyPoisoned: words(lang, 'Enemy poisoned', '敌人中毒', '敵人中毒'),
        };
        return words(lang, reason, `当前未满足条件：${labels[condition[1]]}。`, `目前未滿足條件：${labels[condition[1]]}。`);
      }
      if (reason === 'read only while its flask or charm effect is admitted in the selected combat state') {
        return words(lang, reason, '当前战斗状态未启用此药剂或护符效果。', '目前戰鬥狀態未啟用此藥劑或護符效果。');
      }
      if (reason === 'read only while the equipped flask can apply instant recovery') {
        return words(lang, reason, '当前药剂无法应用立即回复效果。', '目前藥劑無法套用立即回復效果。');
      }
    }
    const missing = /(?:read only while )?the equipped (body armour|shield) has no base (Armour|Evasion|Energy Shield)$/.exec(reason);
    if (!missing || lang === 'en-US') return reason;
    const slot = missing[1] === 'shield' ? '盾牌' : words(lang, 'Body armour', '胸甲', '胸甲');
    const defence = { Armour: words(lang, 'Armour', '护甲', '護甲'), Evasion: words(lang, 'Evasion', '闪避', '閃避'),
      'Energy Shield': words(lang, 'Energy Shield', '能量护盾', '能量護盾') }[missing[2]];
    return words(lang, reason, `当前${slot}没有可缩放的${defence}基值。`, `目前${slot}沒有可縮放的${defence}基值。`);
  });
  return [...new Set(localized)].join(' · ') || null;
}

export interface LedgerFilter {
  /** Show only these statuses; null shows all. */
  statuses: ReadonlySet<LineUsageStatus> | null;
  showInternal?: boolean;
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
  if (filter.showInternal === false && isInternalLine(line)) return false;
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

/** Generated identifiers without imported modifier prose belong in advanced entries. */
export function isInternalLine(line: LineUsageLine): boolean {
  return !line.text || /^[A-Za-z][A-Za-z0-9_.:]*$/.test(cleanLineText(line.text));
}

export function reasonConfigField(line: LineUsageLine, conditionConfigs: Readonly<Record<string, string>> = {}): string | null {
  for (const mod of line.modifiers) {
    if (mod.read_gate?.config_option) return mod.read_gate.config_option;
  }
  const reasons = [line.reason, ...line.modifiers.map(mod => mod.reason)];
  for (const reason of reasons) {
    const condition = reason && /^Condition (\w+) not met$/.exec(reason);
    if (condition && conditionConfigs[condition[1]]) return conditionConfigs[condition[1]];
  }
  return null;
}

export function displayedLedgerReason(line: LineUsageLine, lang: Lang, conditions: Readonly<Record<string, string>> = {}): { text: string | null; untranslated: boolean } {
  const raw = line.reason ? [line.reason] : line.modifiers.flatMap(mod => mod.reason ? [mod.reason] : []);
  let untranslated = false;
  const text = [...new Set(raw)].map(reason => {
    const mapped = plainModifierReason(reason, lang, conditions);
    if (mapped) return mapped;
    const legacy = ledgerReason({ ...line, reason }, lang);
    if (legacy && legacy !== reason) return legacy;
    untranslated = true;
    return words(lang, 'Reason has no translation; open Details.', '原因暂无译文，请展开详情查看原文。', '原因暫無譯文，請展開詳情查看原文。');
  }).join(' · ');
  return { text: text || null, untranslated };
}
