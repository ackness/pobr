import type { PassivePlan } from '../../lib/passivePlanner';

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
  // An end counts when it is part of the change or of the tree the change joins.
  const joins = (changed: ReadonlySet<number>, from: number, to: number) =>
    (changed.has(from) || changed.has(to))
    && (changed.has(from) || allocated.has(from))
    && (changed.has(to) || allocated.has(to));
  return (from, to) => joins(add, from, to) ? ' edge-plan-add' : joins(remove, from, to) ? ' edge-plan-remove' : '';
}
