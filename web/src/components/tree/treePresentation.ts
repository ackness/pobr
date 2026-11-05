import type { AttributeChoice, PassiveJewelState, PassiveNode, WeaponSwap } from '../../api/types';
import { messages, type Lang } from '../../lib/i18n';
import { statFormat } from '../../lib/statDisplay';

/** Gains are differences: percentage formats use percentage points, never relative percent changes. */
export function powerGain(value: number | null, stat: string, lang: Lang): string {
  if (value === null) return '—';
  const text = messages(lang, 'nodePower');
  const format = statFormat(stat);
  const percent = ['percent', 'chanceMultiplier', 'modifier'].includes(format);
  const amount = format === 'chanceMultiplier' || format === 'modifier' ? value * 100 : value;
  const number = amount.toLocaleString(lang, { maximumFractionDigits: 2, signDisplay: 'always' });
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

/** Match the tree editor's ordinary-point budget: class/ascendancy starts are free and ascendancy is separate. */
export function passivePointSummary(nodes: PassiveNode[], allocated: number[], level: number, swap?: WeaponSwap | null, effects?: PassiveJewelState) {
  const byId = new Map(nodes.map(node => [node.skill, node]));
  const exclusive = swap?.exclusive_nodes ?? [[], []];
  const owned = new Set(exclusive.flat());
  const shared = [...new Set(allocated)].filter(id => !owned.has(id));
  const known = [...new Set([...shared, ...exclusive.flat()])].every(id => byId.has(id));
  const starts = new Set(Object.values(effects?.class_starts ?? {}));
  const regular = shared.filter(id => {
    const node = byId.get(id)!;
    return node && !node.ascendancy_id && !starts.has(id) && !['MARAUDER', 'RANGER', 'WITCH', 'DUELIST', 'SIX', 'TEMPLAR', 'SEVEN'].includes(node.name ?? '') && node.kind !== 'ascendancy_start';
  }).length;
  const granted = shared.flatMap(id => byId.get(id)?.stats ?? []).reduce((sum, line) =>
    sum + Number(/^Grants (\d+) Passive Skill Points?$/.exec(line)?.[1] ?? 0), 0);
  return { used: known ? regular + Math.max(exclusive[0].length, exclusive[1].length) : null,
    max: Math.max(0, level - 1) + 24 + granted, weapon: exclusive.map(ids => ids.length) };
}
