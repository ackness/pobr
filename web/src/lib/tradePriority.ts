import type { GemPlan } from './tradeMarket';
import type { TradeOptimization } from './tradeOptimizer';
import { compareObjectiveStats, feasibleOf, minimumDeficit, scoreOf, type Objective } from './optimize';

export interface PositionAnalysis {
  category?: string;
  weights?: TradeOptimization;
  gems?: GemPlan[];
  error?: string;
}

/** Compare complete replacements against the same build, never sums of affix gains. */
export function rankUpgradePositions(results: Record<string, PositionAnalysis>, objective: Objective, damageStat = 'CombinedDPS') {
  return Object.entries(results).flatMap(([slot, result]) => {
    const best = result.weights?.combinations[0];
    const gem = result.gems?.filter(plan => plan.stats && plan.baseline)
      .sort((a, b) => compareObjectiveStats(a.stats!, b.stats!, objective))[0];
    const stats = best?.stats ?? gem?.stats;
    const baseline = result.weights?.baseline ?? gem?.baseline;
    if (!stats || !baseline || !feasibleOf(stats, objective) || compareObjectiveStats(stats, baseline, objective) >= 0) return [];
    const originalScore = scoreOf(baseline, objective);
    const gain = scoreOf(stats, objective) - originalScore;
    if (!Number.isFinite(gain)) return [];
    const balanced: Objective = { stat: damageStat, secondaryStat: 'TotalEHP', constraints: [] };
    const balancedBefore = scoreOf(baseline, balanced);
    const percent = (stat: string) => baseline[stat] > 0 && stats[stat] !== undefined
      ? (stats[stat] - baseline[stat]) / baseline[stat] * 100 : undefined;
    return [{ slot, gain, gainPercent: originalScore !== 0 ? gain / Math.abs(originalScore) * 100 : undefined,
      balancedPercent: balancedBefore > 0 && stats[damageStat] !== undefined && stats.TotalEHP !== undefined
        ? (scoreOf(stats, balanced) - balancedBefore) / balancedBefore * 100 : undefined,
      dpsPercent: percent(damageStat), ehpPercent: percent('TotalEHP'),
      lifeDelta: (stats.Life ?? 0) - (baseline.Life ?? 0),
      dpsDelta: (stats[damageStat] ?? 0) - (baseline[damageStat] ?? 0),
      ehpDelta: (stats.TotalEHP ?? 0) - (baseline.TotalEHP ?? 0),
      deficit: minimumDeficit(stats, objective),
      reference: best?.text ?? '', limited: result.weights?.limited ?? true }];
  }).sort((a, b) => a.deficit - b.deficit || b.gain - a.gain || a.slot.localeCompare(b.slot));
}
