import { describe, expect, it } from 'vitest';
import { hasReusableTradeAnalysis, hasUsableTradeWeights, reusableTradeCount, type CachedTradeAnalysis } from './tradeAnalysis';

describe('trade analysis reuse', () => {
  it('requires combination analysis for a position in the whole-build overview', () => {
    const single = { result: { weights: { weighted: [] } }, mode: 'single' as const };
    const all = { result: { weights: { weighted: [] } }, mode: 'all' as const };
    expect(hasReusableTradeAnalysis('ring1', single, 'single')).toBe(true);
    expect(hasReusableTradeAnalysis('ring1', single, 'all')).toBe(false);
    expect(hasReusableTradeAnalysis('ring1', all, 'all')).toBe(true);
  });

  it('resumes only successful completed positions and accepts gem results from either mode', () => {
    const cache: Record<string, CachedTradeAnalysis<{ weights?: unknown; gems?: unknown; error?: string }>> = {
      ring1: { result: { weights: {} }, mode: 'all' as const },
      ring2: { result: { error: 'failed' }, mode: 'all' as const },
      gems: { result: { gems: [] }, mode: 'single' as const },
    };
    expect(reusableTradeCount(['ring1', 'ring2', 'gems', 'amulet'], cache, 'all')).toBe(2);
  });
});


it('does not enable trade search for empty, zero or invalid weights', () => {
  const weight = (value: number) => ({ id: 'explicit.test', line: '+10 to maximum Life', value: 10, weight: value });
  expect(hasUsableTradeWeights(undefined)).toBe(false);
  expect(hasUsableTradeWeights([])).toBe(false);
  for (const value of [0, 0.0001, NaN, Infinity]) expect(hasUsableTradeWeights([weight(value)])).toBe(false);
  expect(hasUsableTradeWeights([weight(1.5)])).toBe(true);
  expect(hasUsableTradeWeights([weight(-1)])).toBe(true);
});
