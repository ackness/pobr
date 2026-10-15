import type { PassiveNode } from '../api/types';

export const NODE_RADIUS: Record<string, number> = {
  normal: 40, notable: 65, keystone: 90, mastery: 50, jewel_socket: 55, ascendancy_start: 60,
};
export interface ViewBox { x: number; y: number; w: number; h: number }

/** Keep the circular tree reachable when panning or zooming out. */
export function clampView(v: ViewBox, extent: ViewBox): ViewBox {
  const tcx = extent.x + extent.w / 2, tcy = extent.y + extent.h / 2;
  const radius = Math.min(extent.w, extent.h) / 2;
  const half = Math.max(v.w, v.h) / 2;
  const maxOff = Math.max(0, radius - half * 0.5);
  const dx = v.x + v.w / 2 - tcx, dy = v.y + v.h / 2 - tcy;
  const dist = Math.hypot(dx, dy);
  if (dist <= maxOff || dist === 0) return v;
  return { ...v, x: tcx + dx / dist * maxOff - v.w / 2, y: tcy + dy / dist * maxOff - v.h / 2 };
}

export function isAttributeNode(node: PassiveNode): boolean {
  return node.name === 'Attribute' && (node.stats ?? []).some(line => line.includes('any') && line.includes('Attribute'));
}

export function hasPosition(node: PassiveNode): node is PassiveNode & { x: number; y: number } {
  return typeof node.x === 'number' && Number.isFinite(node.x) && typeof node.y === 'number' && Number.isFinite(node.y);
}

export function treeExtent(nodes: PassiveNode[], padding = 400, minimum = 0): ViewBox | null {
  const placed = nodes.filter(hasPosition);
  if (!placed.length) return null;
  const xs = placed.map(node => node.x), ys = placed.map(node => node.y);
  const minX = Math.min(...xs), maxX = Math.max(...xs), minY = Math.min(...ys), maxY = Math.max(...ys);
  const w = Math.max(minimum, maxX - minX + padding * 2), h = Math.max(minimum, maxY - minY + padding * 2);
  return { x: (minX + maxX - w) / 2, y: (minY + maxY - h) / 2, w, h };
}

/** Connections are one-way records. Deduplicate undirected pairs, not by source ID order. */
export function passiveTreeEdges(nodes: PassiveNode[]) {
  const placed = nodes.filter(hasPosition), byId = new Map(placed.map(node => [node.skill, node]));
  const seen = new Set<string>();
  const edges: { from: number; to: number; x1: number; y1: number; x2: number; y2: number }[] = [];
  for (const node of placed) for (const target of node.connections ?? []) {
    const other = byId.get(target);
    if (!other || (node.ascendancy_id ?? null) !== (other.ascendancy_id ?? null)) continue;
    const key = [node.skill, target].sort((a, b) => a - b).join(':');
    if (seen.has(key)) continue;
    seen.add(key);
    edges.push({ from: node.skill, to: target, x1: node.x, y1: node.y, x2: other.x, y2: other.y });
  }
  return edges;
}
