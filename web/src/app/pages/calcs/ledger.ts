import { plainModifierReason } from '../../../lib/diagnosticText';
import { cleanLineText } from '../../../lib/modifierText';
import { bindT, formatMessage, originLabel, t, translated, type Lang, type UiKey } from '../../../lib/i18n';
import { statRowLabel } from '../../../lib/statDisplay';
import type { LineUsage, LineUsageLine, LineUsageStatus } from '../../../api/types';

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
  const tt = bindT(lang);
  switch (status) {
    case 'applied': return tt('ledger.status.applied');
    case 'partial': return tt('ledger.status.partial');
    case 'inactive': return tt('ledger.status.inactive');
    case 'unread': return tt('ledger.status.unread');
    case 'outside': return tt('ledger.status.outside');
    case 'no_effect': return tt('ledger.status.noEffect');
    case 'unmodelled': return tt('ledger.status.unmodelled');
    case 'unparsed': return tt('ledger.status.unparsed');
    case 'no_modifier': return tt('ledger.status.noModifier');
  }
}

export function statusHint(lang: Lang, status: LineUsageStatus): string {
  const tt = bindT(lang);
  switch (status) {
    case 'applied': return tt('ledger.hint.applied');
    case 'partial': return tt('ledger.hint.partial');
    case 'inactive': return tt('ledger.hint.inactive');
    case 'unread': return tt('ledger.hint.unread');
    case 'outside': return tt('ledger.hint.outside');
    case 'no_effect': return tt('ledger.hint.noEffect');
    case 'unmodelled': return tt('ledger.hint.unmodelled');
    case 'unparsed': return tt('ledger.hint.unparsed');
    case 'no_modifier': return tt('ledger.hint.noModifier');
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
  const tt = bindT(lang);
  switch (group) {
    case 'item': return tt('calcs.sourceEquipment');
    case 'tree': return tt('ledger.group.tree');
    case 'jewel': return tt('ledger.group.jewel');
    case 'flask': return tt('ledger.group.flask');
    case 'config': return tt('ledger.group.config');
    case 'skill': return tt('tab.skills');
    case 'other': return tt('ledger.group.other');
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
      const label = resolved === line.source_id ? t(lang, 'ledger.unnamedSetting') : resolved;
      return t(lang, 'ledger.configuration', { label });
    }
  }
  if (line.source_id === 'base.AncestralBoost'
    && text === 'Ancestral Boost: 2 additional Strike targets; 30% more Slam damage and 25% increased Slam area') {
    return t(lang, 'ledger.ancestralBoost');
  }
  return text;
}

/** Explain an inactive row without making the user expand its modifier table. */
export function ledgerReason(line: LineUsageLine, lang: Lang): string | null {
  const reasons = line.reason ? [line.reason] : line.modifiers
    .filter(mod => mod.status === 'inactive' || mod.status === 'unread')
    .flatMap(mod => mod.reason ? [mod.reason] : []);
  const copy = (key: string) => translated(lang, 'ledgerLegacyReason', key);
  // These display explanations supplement the engine's original English reasons.
  const conditionKeys: Record<string, UiKey> = {
    Combat: 'condition.combat',
    Buffs: 'condition.buffs',
    Effective: 'condition.effective',
    AncestralBoost: 'condition.ancestralBoost',
    EnemyPoisoned: 'condition.enemyPoisoned',
  };
  const localized = [...new Set(reasons)].map(reason => {
    if (reason === 'its value is 0') return copy('zero') ?? reason;
    if (/^SkillType \w+(?: · SkillType \w+)*$/.test(reason)) return copy('skillType') ?? reason;
    if (line.status === 'unmodelled' && /^rule \S+ recognises it and stores nothing$/.test(reason)) {
      return translated(lang, 'ui', 'ledger.hint.unmodelled') ?? reason;
    }
    if (reason === 'a config option nothing in this build asks for') return copy('unusedOption') ?? reason;
    const condition = /^Condition (Combat|Buffs|Effective|AncestralBoost|EnemyPoisoned) not met$/.exec(reason);
    const conditionTemplate = condition && copy('condition');
    if (condition && conditionTemplate) return formatMessage(conditionTemplate, { condition: t(lang, conditionKeys[condition[1]]) });
    if (reason === 'read only while its flask or charm effect is admitted in the selected combat state') {
      return copy('inactiveUtility') ?? reason;
    }
    if (reason === 'read only while the equipped flask can apply instant recovery') return copy('instantFlask') ?? reason;
    const missing = /(?:read only while )?the equipped (body armour|shield) has no base (Armour|Evasion|Energy Shield)$/.exec(reason);
    const missingTemplate = missing && copy('missingDefence');
    if (!missing || !missingTemplate) return reason;
    const slot = t(lang, missing[1] === 'shield' ? 'trade.category.armour.shield' : 'trade.category.armour.chest');
    const defence = statRowLabel(lang, missing[2].replace(' ', ''));
    return formatMessage(missingTemplate, { slot, defence });
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
    return t(lang, 'calcs.unknownReason');
  }).join(' · ');
  return { text: text || null, untranslated };
}
