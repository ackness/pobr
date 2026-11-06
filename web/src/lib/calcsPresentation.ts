import type { Breakdown, BreakdownMod, BreakdownModType } from '../api/types';
import { sourceGroup, type SourceGroup } from '../app/pages/calcs/ledger';

export interface ModifierMeasure { stat: string; unit: string }
export interface ModGroup { source: SourceGroup; mods: BreakdownMod[]; subtotals: { type: BreakdownModType; value: number }[] }
/** Source families use real attribution. Subtotals require a caller-confirmed single stat and unit;
 * raw BreakdownMod rows do not provide that identity, so their subtotals are omitted. */
export function groupModifiers(mods: readonly BreakdownMod[], measure?: ModifierMeasure): ModGroup[] {
  const groups = new Map<SourceGroup, BreakdownMod[]>();
  for (const mod of mods) {
    const source = sourceGroup({ source_id: mod.origin_id, source_kind: mod.origin_kind });
    const rows = groups.get(source) ?? [];
    rows.push(mod); groups.set(source, rows);
  }
  return [...groups].map(([source, rows]) => ({ source, mods: rows, subtotals: (['BASE', 'INC', 'MORE'] as const).flatMap(type => {
    const active = rows.filter(mod => mod.active && mod.mod_type === type && mod.value !== null);
    if (!measure?.stat || !measure.unit || !active.length) return [];
    return [{ type, value: active.reduce((total, mod) => type === 'MORE' ? total * (1 + mod.value! / 100) : total + mod.value!, type === 'MORE' ? 1 : 0) }];
  }) }));
}

/** Formula operands already encode the applied rates; do not reconstruct missing links. */
export function damageChain(breakdown: Breakdown) {
  const steps = breakdown.formula?.steps ?? [];
  const find = (op: string) => steps.find(step => step.op === op);
  const average = find('average_hit');
  const enemy = find('enemy_mitigation');
  const uses = find('uses_per_second');
  const trigger = find('energy_trigger');
  return [
    { key: 'calcs.chainHit', value: average?.result, unit: 'damage' },
    { key: 'calcs.chainEnemy', value: enemy?.result ?? (breakdown.mitigation?.length ? breakdown.mitigation.reduce((sum, row) => sum + row.mitigated, 0) : undefined), unit: 'damage' },
    { key: 'calcs.chainRate', value: uses?.operand, unit: 'rate' },
    ...(find('chance_to_hit') ? [{ key: 'calcs.step.chance_to_hit' as const, value: find('chance_to_hit')!.operand, unit: 'percent' as const }] : []),
    ...(find('dps_multiplier') ? [{ key: 'calcs.step.dps_multiplier' as const, value: find('dps_multiplier')!.operand, unit: 'multiplier' as const }] : []),
    ...(find('reload') ? [{ key: 'calcs.step.reload' as const, value: find('reload')!.operand, unit: 'multiplier' as const }] : []),
    { key: 'calcs.chainTrigger', value: trigger?.operand, unit: 'multiplier' },
  ] as const;
}

export function resistanceIds(stat: string) {
  return /^(Fire|Cold|Lightning|Chaos)Resist$/.test(stat) ? [stat + 'Total', stat + 'Max', stat + 'OverCap'] : [];
}
