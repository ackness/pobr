import { describe, expect, test } from 'vitest';
import { plannedEdgeClass, routeViewBox } from './plannedRoute';

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
    expect(edge(3, 10)).toBe('');
    expect(edge(1, 2)).toBe('');
  });

  test('a refunded point is not joined to a point that is not allocated', () => {
    const edge = plannedEdgeClass({ allocate: [], deallocate: [3] }, allocated);
    expect(edge(3, 50)).toBe('');
  });
});

describe('routeViewBox', () => {
  const nodes = [
    { skill: 1, id: 'start', kind: 'normal', x: -6600, y: 400 },
    { skill: 2, id: 'end', kind: 'normal', x: 7010, y: -3200 },
  ] as const;

  test.each([[1208, 988, 114, 56], [568, 702, 144, 126], [390, 523, 72, 0]])(
    'fits a distant exchange with padding inside a %ipx by %ipx canvas', (width, height, top, bottom) => {
      const view = routeViewBox([...nodes], width, height, top, bottom)!;
      expect(view.w / view.h).toBeCloseTo(width / height);
      for (const node of nodes) {
        const x = (node.x - view.x) * width / view.w;
        const y = (node.y - view.y) * height / view.h;
        expect(x).toBeGreaterThan(30);
        expect(x).toBeLessThan(width - 30);
        expect(y).toBeGreaterThan(top + 22);
        expect(y).toBeLessThan(height - bottom - 22);
      }
    },
  );

  test('centers on a single off-center node without the ordinary pan limit', () => {
    const node = nodes[1];
    const view = routeViewBox([node], 390, 523, 72, 0)!;
    expect((node.x - view.x) * 390 / view.w).toBeCloseTo(195);
    expect((node.y - view.y) * 523 / view.h).toBeCloseTo((523 + 72) / 2);
  });
});
