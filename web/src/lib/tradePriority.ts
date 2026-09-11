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
export function rankUpgradePositions(results: Record<string, PositionAnalysis>, objective: Objective) {
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
    return [{ slot, gain, gainPercent: originalScore !== 0 ? gain / Math.abs(originalScore) * 100 : undefined,
      lifeDelta: (stats.Life ?? 0) - (baseline.Life ?? 0),
      dpsDelta: (stats.TotalDPS ?? 0) - (baseline.TotalDPS ?? 0),
      ehpDelta: (stats.TotalEHP ?? 0) - (baseline.TotalEHP ?? 0),
      deficit: minimumDeficit(stats, objective),
      reference: best?.text ?? '', limited: result.weights?.limited ?? true }];
  }).sort((a, b) => a.deficit - b.deficit || b.gain - a.gain || a.slot.localeCompare(b.slot));
}
