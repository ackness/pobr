import { describe, expect, it } from 'vitest';
import { ENGINE_DEFAULT_TIER, resolveEnemyTier } from './enemy';

describe('resolveEnemyTier', () => {
  it('uses the tier when one is set', () => {
    expect(resolveEnemyTier('boss', {})).toBe('boss');
    expect(resolveEnemyTier('none', { enemyIsBoss: 'Standard' })).toBe('none');
  });
  it('is the engine default (Pinnacle) when nothing says otherwise', () => {
    expect(ENGINE_DEFAULT_TIER).toBe('pinnacle');
    expect(resolveEnemyTier(undefined, {})).toBe('pinnacle');
    expect(resolveEnemyTier(undefined, undefined)).toBe('pinnacle');
  });
  it('is custom when the BD carries an enemy input the app does not recognise', () => {
    expect(resolveEnemyTier(undefined, { enemyIsBoss: 'Standard' })).toBe('custom');
  });
});
