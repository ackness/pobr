export type TradeAnalysisMode = 'single' | 'all';

export interface CachedTradeAnalysis<T> {
  result: T;
  mode: TradeAnalysisMode;
}

/** A single-position affix score lacks the combination search used by the overview. */
export function hasReusableTradeAnalysis<T extends { weights?: unknown; gems?: unknown; error?: string }>(
  slot: string,
  cached: CachedTradeAnalysis<T> | undefined,
  mode: TradeAnalysisMode,
): boolean {
  if (!cached || cached.result.error) return false;
  if (slot === 'gems') return cached.result.gems !== undefined;
  return cached.result.weights !== undefined && (mode === 'single' || cached.mode === 'all');
}

export function reusableTradeCount<T extends { weights?: unknown; gems?: unknown; error?: string }>(
  targets: string[],
  cache: Record<string, CachedTradeAnalysis<T>>,
  mode: TradeAnalysisMode,
): number {
  return targets.filter(slot => hasReusableTradeAnalysis(slot, cache[slot], mode)).length;
}
