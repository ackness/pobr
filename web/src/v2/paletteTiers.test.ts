import { describe, expect, it } from 'vitest';
import { matchingTiers } from './paletteTiers';

describe('matchingTiers', () => {
  it('lists all four enemies for an empty query, normal enemy first', () => {
    expect(matchingTiers('', 'en-US').map(entry => entry.tier)).toEqual(['none', 'boss', 'pinnacle', 'uber']);
  });
  it('puts the plain boss first when you type boss, and never the normal enemy', () => {
    const tiers = matchingTiers('boss', 'en-US').map(entry => entry.tier);
    expect(tiers[0]).toBe('boss');
    expect(tiers).not.toContain('none');
  });
  it('finds a single enemy by its own name, in Chinese too', () => {
    expect(matchingTiers('uber', 'en-US').map(entry => entry.tier)).toEqual(['uber']);
    expect(matchingTiers('刷', 'zh-CN').length).toBeLessThanOrEqual(1);
  });
});
