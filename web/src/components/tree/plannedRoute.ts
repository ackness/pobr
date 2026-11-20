import type { PassiveNode } from '../../api/types';
import type { PassivePlan } from '../../lib/passivePlanner';
import { hasPosition, treeExtent, type ViewBox } from '../../lib/treeGeometry';

export type RouteScope = 'all' | 'add' | 'remove';

export function placedRouteNodes(ids: number[], byId: ReadonlyMap<number, PassiveNode>) {
  return ids.map(id => byId.get(id)).filter(node => node !== undefined && hasPosition(node));
}

/** Fit the point bounds to the unobstructed portion of the SVG at its current aspect ratio. */
export function routeViewBox(nodes: PassiveNode[], width: number, height: number, top: number, bottom: number): ViewBox | null {
  const bounds = treeExtent(nodes, 130);
  if (!bounds || width <= 0 || height <= 0) return null;
  const freeHeight = height - top - bottom;
  const scale = Math.min(0.38, Math.max(1, width - 64) / bounds.w, Math.max(1, freeHeight - 48) / bounds.h);
  const w = width / scale;
  const h = height / scale;
  return {
    x: bounds.x + bounds.w / 2 - w / 2,
    y: bounds.y + bounds.h / 2 - (freeHeight > 0 ? top + freeHeight / 2 : height / 2) / scale,
    w,
    h,
  };
}

/**
 * The class a tree edge takes while a planner route is shown: a new point joins the
 * allocated tree through it, or a refunded branch is cut from the tree along it.
 * Edges the route does not touch keep their own look, and so do the edges between
 * two points that stay allocated.
 */
export function plannedEdgeClass(
  plan: Pick<PassivePlan, 'allocate' | 'deallocate'> | null,
  allocated: ReadonlySet<number>,
): (from: number, to: number) => string {
  if (!plan) return () => '';
  const add = new Set(plan.allocate);
  const remove = new Set(plan.deallocate);
  // Only show edges that exist on the proposed route or are cut from the current tree.
  const added = (id: number) => add.has(id) || (allocated.has(id) && !remove.has(id));
  return (from, to) => {
    if ((add.has(from) || add.has(to)) && added(from) && added(to)) return ' edge-plan-add';
    if ((remove.has(from) || remove.has(to)) && allocated.has(from) && allocated.has(to)) return ' edge-plan-remove';
    return '';
  };
}
