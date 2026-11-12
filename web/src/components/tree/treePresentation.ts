import { OBJECTIVE_PRESETS } from '../../lib/optimize';
import { formatComparisonDelta } from '../../lib/compare';
import type { CalculateBuildResponse, AttributeChoice, PassiveClass, PassiveJewelState, PassiveNode, TreeAllocationMode, TreeEditResult, WeaponSwap } from '../../api/types';
import { implicitClassStarts } from '../../lib/passiveGraph';
import { bindT, messages, statNameLabel, type Lang } from '../../lib/i18n';
import { formatNumber, statFormat } from '../../lib/statDisplay';

/** The choice is already UI text; append it after translating the node's own stats. */
export function treeHoverLines(stats: string[], choice: AttributeChoice | undefined, lang: Lang): string[] {
  return choice ? [...stats, `→ ${bindT(lang)(`tree.attr.${choice}`)}`] : stats;
}

/** Gains are differences: percentage formats use percentage points, never relative percent changes. */
export function powerGain(value: number | null, stat: string, lang: Lang): string {
  if (value === null) return '—';
  const text = messages(lang, 'nodePower');
  const format = statFormat(stat);
  const percent = ['percent', 'chanceMultiplier', 'modifier'].includes(format);
  const amount = format === 'chanceMultiplier' || format === 'modifier' ? value * 100 : value;
  const number = `${amount >= 0 ? '+' : ''}${formatNumber(amount)}`;
  const unit = percent ? text.percentUnit : format === 'rate' || /DPS$/.test(stat) ? text.rateUnit : text.valueUnit;
  return `${number} ${unit}`;
}

/** Deterministic distribution retains choices outside the currently allocated attribute passives. */
export function distributeAttributeChoices(ids: number[], previous: Record<string, AttributeChoice>, counts: Record<AttributeChoice, number>) {
  const next = Object.fromEntries(Object.entries(previous).filter(([id]) => !ids.includes(Number(id))));
  [...ids].sort((a, b) => a - b).forEach((id, index) => {
    if (index < counts.str) next[id] = 'str';
    else if (index < counts.str + counts.dex) next[id] = 'dex';
    else if (index < counts.str + counts.dex + counts.int) next[id] = 'int';
  });
  return next;
}

/** Saved choices take precedence; class base attributes supply the default travel attribute. */
export function defaultTravelAttribute(choices: Record<string, AttributeChoice>, characterClass?: PassiveClass): AttributeChoice {
  const counts = { str: 0, dex: 0, int: 0 };
  for (const choice of Object.values(choices)) counts[choice] += 1;
  const attributes: AttributeChoice[] = ['str', 'dex', 'int'];
  if (Object.values(counts).some(Boolean)) return attributes.sort((a, b) => counts[b] - counts[a])[0];
  // Metadata can be absent in an older response. Use a neutral UI default then.
  if (!characterClass) return 'dex';
  const base = { str: characterClass.base_str, dex: characterClass.base_dex, int: characterClass.base_int };
  // Hybrid classes keep a deterministic preference when base attributes tie.
  return (['dex', 'int', 'str'] as AttributeChoice[]).sort((a, b) => base[b] - base[a])[0];
}

/** Empty trees probe a regular neighbor in a weapon set, leaving shared point grants untouched. */
export function passivePointSummaryTarget(nodes: readonly PassiveNode[], allocated: ReadonlySet<number>,
  starts: ReadonlySet<number>, neighbours: readonly number[], swap?: WeaponSwap | null): { node: number | null; mode: TreeAllocationMode } {
  const existing = nodes.find(node => allocated.has(node.skill) && !starts.has(node.skill) && node.kind !== 'ascendancy_start');
  if (existing) {
    const set = swap?.exclusive_nodes.findIndex(ids => ids.includes(existing.skill)) ?? -1;
    return { node: existing.skill, mode: (set + 1) as TreeAllocationMode };
  }
  if (allocated.size || swap?.exclusive_nodes.some(ids => ids.length)) return { node: null, mode: 0 };
  const adjacent = new Set(neighbours);
  const probe = nodes.find(node => adjacent.has(node.skill) && !starts.has(node.skill)
    && node.kind === 'normal' && !node.ascendancy_id && !node.unlock_constraint);
  return { node: probe?.skill ?? null, mode: 1 };
}

/** Budgets belong to the engine; an empty tree may borrow its budget from a weapon-only probe. */
export function passivePointSummary(nodes: PassiveNode[], allocated: number[], swap?: WeaponSwap | null,
  effects?: PassiveJewelState, current?: TreeEditResult | null) {
  if (current && !current.added.length && !current.removed.length && !current.promoted.length) {
    return { used: current.points_used, max: current.points_max, weapon: current.weapon_points_used };
  }
  const byId = new Map(nodes.map(node => [node.skill, node]));
  const exclusive = swap?.exclusive_nodes ?? [[], []];
  const owned = new Set(exclusive.flat());
  const shared = [...new Set(allocated)].filter(id => !owned.has(id));
  const all = [...new Set([...shared, ...exclusive.flat()])];
  // The engine grants ordinary points only from shared nodes. An exclusive-only
  // probe therefore has the same maximum as an empty tree, even if its node grants points.
  const emptyBudget = !all.length && current && !current.allocation.shared.length
    && !current.removed.length && !current.promoted.length ? current.points_max : null;
  const known = all.every(id => byId.has(id)) && (!all.length || Object.keys(effects?.class_starts ?? {}).length > 0);
  const starts = implicitClassStarts(effects);
  const regular = shared.filter(id => {
    const node = byId.get(id)!;
    return node && !node.ascendancy_id && !starts.has(id) && node.kind !== 'ascendancy_start';
  }).length;
  return { used: known ? regular + Math.max(exclusive[0].length, exclusive[1].length) : null,
    max: emptyBudget, weapon: exclusive.map(ids => ids.length) };
}

export function treeStatLabel(lang: Lang, stat: string): string {
  return stat === 'TotalDPS' ? { 'en-US': 'Hit DPS', 'zh-CN': '击中 DPS', 'zh-TW': '擊中 DPS' }[lang] : statNameLabel(lang, stat);
}

/** Missing and unbounded outputs are not zero changes. */
export function treeStatDelta(before: CalculateBuildResponse | null, after: CalculateBuildResponse | null, stat: string): number | null {
  const a = before?.stats.find(row => row.id === stat)?.value;
  const b = after?.stats.find(row => row.id === stat)?.value;
  return typeof a === 'number' && typeof b === 'number' && Number.isFinite(a) && Number.isFinite(b) ? b - a : null;
}

export function refundDelta(before: CalculateBuildResponse | null, after: CalculateBuildResponse | null, lang: Lang): string {
  const delta = treeStatDelta(before, after, 'CombinedDPS');
  if (delta === null) return bindT(lang)('tree.deltaUnavailable');
  const baseline = before!.stats.find(stat => stat.id === 'CombinedDPS')!.value!;
  const absolute = formatComparisonDelta(delta, 'float2', lang);
  const percent = `${delta >= 0 ? '+' : ''}${formatNumber(delta / baseline * 100, 1)}`;
  return baseline > 0 ? `${absolute}${lang === 'en-US' ? ` (${percent}%)` : `（${percent}%）`}` : absolute;
}

/** The report follows the player's current optimization goal until they choose a report metric. */
export function defaultReportStat(preset: string, available: string[]): string {
  let goal = OBJECTIVE_PRESETS.find(entry => entry.id === preset)?.stat;
  // A summoner build's damage is its main minion's.
  if ((goal === 'TotalDPS' || goal === 'CombinedDPS') && available.includes('MainMinionDPS')) goal = 'MainMinionDPS';
  return goal && available.includes(goal) ? goal : available[0] ?? 'Life';
}
