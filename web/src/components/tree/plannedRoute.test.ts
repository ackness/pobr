import { describe, expect, test } from 'vitest';
import { plannedEdgeClass } from './plannedRoute';

describe('plannedEdgeClass', () => {
  const allocated = new Set([1, 2, 3, 4]);

  test('marks nothing without a route', () => {
    expect(plannedEdgeClass(null, allocated)(1, 2)).toBe('');
  });

  test('marks the edges a new route joins the tree by', () => {
    const edge = plannedEdgeClass({ allocate: [10, 11], deallocate: [] }, allocated);
    expect(edge(4, 10)).toBe(' edge-plan-add');
    expect(edge(10, 11)).toBe(' edge-plan-add');
    expect(edge(11, 10)).toBe(' edge-plan-add');
    // Points that stay allocated, and points the route does not use, are left alone.
    expect(edge(1, 2)).toBe('');
    expect(edge(11, 99)).toBe('');
    expect(edge(98, 99)).toBe('');
  });

  test('marks the branch a respec gives back, separately from the route that replaces it', () => {
    const edge = plannedEdgeClass({ allocate: [10], deallocate: [3, 4] }, allocated);
    expect(edge(2, 3)).toBe(' edge-plan-remove');
    expect(edge(3, 4)).toBe(' edge-plan-remove');
    expect(edge(2, 10)).toBe(' edge-plan-add');
    expect(edge(1, 2)).toBe('');
  });

  test('a refunded point is not joined to a point that is not allocated', () => {
    const edge = plannedEdgeClass({ allocate: [], deallocate: [3] }, allocated);
    expect(edge(3, 50)).toBe('');
  });
});
