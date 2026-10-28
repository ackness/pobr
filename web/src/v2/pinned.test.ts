import { describe, expect, it } from 'vitest';
import { MAX_PINNED, togglePinned } from './pinned';

describe('togglePinned', () => {
  it('pins a new stat last and unpins one that is already pinned', () => {
    expect(togglePinned(['Life'], 'TotalEHP')).toEqual(['Life', 'TotalEHP']);
    expect(togglePinned(['Life', 'TotalEHP'], 'Life')).toEqual(['TotalEHP']);
  });
  it('caps the list, dropping the oldest pin', () => {
    const full = Array.from({ length: MAX_PINNED }, (_, i) => `S${i}`);
    const next = togglePinned(full, 'New');
    expect(next).toHaveLength(MAX_PINNED);
    expect(next.at(-1)).toBe('New');
    expect(next).not.toContain('S0');
  });
});
