import type { Breakdown, BreakdownMod, BreakdownModType, FormulaStep } from '../api/types';
import { sourceGroup, type SourceGroup } from '../app/pages/calcs/ledger';
import type { UiKey } from './i18n';

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

/** The engine scales the action rate after the chain only by reload and energy-trigger steps. */
export function actionRateIsFinal(steps: readonly FormulaStep[]): boolean {
  return !steps.some(step => step.op === 'reload' || step.op === 'energy_trigger');
}

/** Each Combined DPS step adds one final stat; its operand is that stat's value. */
export const COMBINED_DPS_COMPONENTS: Readonly<Record<string, string>> = {
  hit_dps: 'TotalDPS', skill_dot: 'TotalDotDPS', dot_dps: 'TotalDotDPS', poison_dps: 'PoisonDPS', ignite_dps: 'IgniteDPS', bleed_dps: 'BleedDPS',
};

interface DamageChainLink {
  key: UiKey;
  value: number | undefined;
  unit: 'damage' | 'rate' | 'percent' | 'multiplier' | 'dps';
  /** Present only when this value is the named stat's final result. */
  stat?: string;
}

/** Formula operands already encode the applied rates; do not reconstruct missing links. */
export function damageChain(breakdown: Breakdown): DamageChainLink[] {
  const steps = breakdown.formula?.steps ?? [];
  const find = (op: string) => steps.find(step => step.op === op);
  const average = find('average_hit');
  const enemy = find('enemy_mitigation');
  const uses = find('uses_per_second');
  const trigger = find('energy_trigger');
  return [
    { key: 'calcs.chainHit', value: average?.result, unit: 'damage', stat: 'TotalHitAvg' },
    // Enemy-adjusted hit excludes hit chance; AverageDamage includes it.
    { key: 'calcs.chainEnemy', value: enemy?.result ?? (breakdown.mitigation?.length ? breakdown.mitigation.reduce((sum, row) => sum + row.mitigated, 0) : undefined), unit: 'damage' },
    // Reload and energy-trigger factors are applied after this rate.
    { key: 'calcs.chainRate', value: uses?.operand, unit: 'rate', stat: actionRateIsFinal(steps) ? 'ActionRate' : undefined },
    ...(find('chance_to_hit') ? [{ key: 'calcs.step.chance_to_hit' as const, value: find('chance_to_hit')!.operand, unit: 'percent' as const, stat: 'HitChance' }] : []),
    // This factor combines DpsMultiplier and QuantityMultiplier, so it is neither stat alone.
    ...(find('dps_multiplier') ? [{ key: 'calcs.step.dps_multiplier' as const, value: find('dps_multiplier')!.operand, unit: 'multiplier' as const }] : []),
    ...(find('reload') ? [{ key: 'calcs.step.reload' as const, value: find('reload')!.operand, unit: 'multiplier' as const }] : []),
    { key: 'calcs.chainTrigger', value: trigger?.operand, unit: 'multiplier' },
    ...(breakdown.formula ? [{ key: 'calcs.hitDps' as const, value: breakdown.formula.value, unit: 'dps' as const, stat: 'TotalDPS' }] : []),
  ] as const;
}

export function resistanceIds(stat: string) {
  return /^(Fire|Cold|Lightning|Chaos)Resist$/.test(stat) ? [stat + 'Total', stat + 'Max', stat + 'OverCap'] : [];
}
