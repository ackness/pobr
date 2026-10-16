import { describe, expect, test } from 'vitest';
import type { PassiveNode } from '../api/types';
import { clampView, hasPosition, isAttributeNode, passiveTreeEdges, treeExtent } from './treeGeometry';

const node = (skill: number, patch: Partial<PassiveNode> = {}): PassiveNode => ({ skill, id: String(skill), kind: 'normal', x: skill * 100, y: 0, ...patch });

describe('shared tree geometry', () => {
  test('keeps high-to-low one-way connections, deduplicates pairs, and rejects cross-region or unplaced targets', () => {
    const nodes = [node(4, { connections: [1, 3, 5, 9] }), node(1, { connections: [4] }),
      node(3, { x: undefined }), node(5, { ascendancy_id: 'Deadeye' })];
    expect(passiveTreeEdges(nodes)).toEqual([{ from: 4, to: 1, x1: 400, y1: 0, x2: 100, y2: 0 }]);
  });
  test('does not invent historical positions and retains centred minimum bounds', () => {
    expect(hasPosition(node(1, { x: NaN }))).toBe(false);
    expect(treeExtent([node(1, { y: undefined })])).toBeNull();
    expect(treeExtent([node(2)], 100, 1000)).toEqual({ x: -300, y: -500, w: 1000, h: 1000 });
    expect(treeExtent([node(1), node(4)], 100)).toEqual({ x: 0, y: -100, w: 500, h: 200 });
  });
  test('recognizes only choice attributes and keeps the editing canvas reachable', () => {
    expect(isAttributeNode(node(1, { name: 'Attribute', stats: ['+5 to any Attribute'] }))).toBe(true);
    expect(isAttributeNode(node(1, { name: 'Strength', stats: ['+10 to Strength'] }))).toBe(false);
    const extent = { x: -1000, y: -1000, w: 2000, h: 2000 };
    expect(clampView({ x: -500, y: -500, w: 1000, h: 1000 }, extent)).toEqual({ x: -500, y: -500, w: 1000, h: 1000 });
    expect(clampView({ x: 10000, y: -500, w: 1000, h: 1000 }, extent)).toEqual({ x: 250, y: -500, w: 1000, h: 1000 });
  });
});
