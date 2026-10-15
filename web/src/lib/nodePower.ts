import { getBackend, type PobrBackend } from '../api/backend';
import type { CalculateBuildRequest, PassiveNode } from '../api/types';
import { allocationRoutes, buildPassiveGraph } from './passiveGraph';
import { allocationPaths, type PassivePlanningContext } from './passivePlanner';
import { evaluateVariants, type EvaluateOptions, type EvaluateResult } from './optimize';

export const NODE_POWER_LIMIT = 128;
const BATCH = 16;
export type PowerStatus = 'ready' | 'pending' | 'unavailable' | 'unsupported' | 'error';
export interface PowerRow {
  node: PassiveNode;
  path: number[] | null;
  status: PowerStatus;
  gain: number | null;
  perPoint: number | null;
  single: number | null;
  diagnostics: string[];
}
export type PowerSort = 'node' | 'kind' | 'points' | 'gain' | 'perPoint' | 'single';

/** List every unallocated candidate, including those the planner cannot route. */
export function powerCandidates(nodes: PassiveNode[], context: PassivePlanningContext, allocated: readonly number[], ascendancy?: string): PowerRow[] {
  const current = new Set(allocated);
  const paths = new Map(allocationPaths(context, context.allocated, nodes.length).map(p => [p.target, p.allocate]));
  const ascNodes = nodes.filter(n => ascendancy && n.ascendancy_id === ascendancy
    && n.kind !== 'mastery' && (!n.unlock_constraint || n.unlock_constraint.nodes.every(id => current.has(id))));
  const ascRoot = ascNodes.find(n => n.kind === 'ascendancy_start')?.skill;
  if (ascRoot !== undefined) {
    const ascAllocated = new Set(ascNodes.filter(n => current.has(n.skill)).map(n => n.skill));
    for (const [id, path] of allocationRoutes(buildPassiveGraph(ascNodes), ascAllocated, ascRoot, [], nodes.length)) {
      if (path.length && id !== ascRoot) paths.set(id, path);
    }
  }
  return nodes.filter(n => !current.has(n.skill) && (!n.ascendancy_id || n.ascendancy_id === ascendancy)
    && ['normal', 'notable', 'keystone'].includes(n.kind) && !context.roots.has(n.skill)
    && (n.kind !== 'normal' || Boolean(n.stats?.length) || n.name === 'Attribute'))
    .map(node => {
      const path = paths.get(node.skill) ?? null;
      return { node, path, status: path ? 'pending' : 'unavailable', gain: null, perPoint: null, single: null, diagnostics: [] };
    });
}

export function filterPowerRows(rows: PowerRow[], search: string, small: boolean): PowerRow[] {
  const query = search.trim().toLocaleLowerCase();
  return rows.filter(row => (small || row.node.kind !== 'normal') && (!query
    || [row.node.name, row.node.id, row.node.skill, ...(row.node.stats ?? [])].join(' ').toLocaleLowerCase().includes(query)));
}

/** Unknown values sort last in either direction; ties are stable by cost then ID. */
export function rankPowerRows(rows: PowerRow[], sort: PowerSort, ascending = false): PowerRow[] {
  const value = (row: PowerRow): string | number | null => sort === 'node' ? row.node.name ?? row.node.id
    : sort === 'kind' ? row.node.kind : sort === 'points' ? row.path?.length ?? null : row[sort];
  return [...rows].sort((a, b) => {
    const av = value(a), bv = value(b);
    if (av === null || bv === null) {
      if (av !== bv) return av === null ? 1 : -1;
    } else {
      const delta = typeof av === 'string' ? av.localeCompare(String(bv)) : av - Number(bv);
      if (delta) return ascending ? delta : -delta;
    }
    return (a.path?.length ?? Infinity) - (b.path?.length ?? Infinity) || a.node.skill - b.node.skill;
  });
}

interface RunOptions {
  request: CalculateBuildRequest;
  rows: PowerRow[];
  stat: string;
  signal?: AbortSignal;
  onBatch?: (rows: PowerRow[], done: number, total: number, baselineLimits: string[]) => void;
  evaluate?: (options: EvaluateOptions) => Promise<EvaluateResult>;
  probe?: PobrBackend['nodePower'];
}

/** Bounded batches reuse both the heatmap probe and the complete-route planner evaluator. */
export async function evaluateNodePower(options: RunOptions): Promise<{ rows: PowerRow[]; cancelled: boolean }> {
  const pending = rankPowerRows(options.rows.filter(r => r.path && r.status === 'pending'), 'points', true).slice(0, NODE_POWER_LIMIT);
  const results: PowerRow[] = [];
  const evaluate = options.evaluate ?? evaluateVariants;
  const probe = options.probe ?? (await getBackend()).nodePower;
  for (let start = 0; start < pending.length; start += BATCH) {
    if (options.signal?.aborted) break;
    const batch = pending.slice(start, start + BATCH);
    const evaluated = await evaluate({ request: options.request, stats: [options.stat],
      variants: [{}, ...batch.map(row => ({ allocate_nodes: row.path! }))], signal: options.signal });
    if (options.signal?.aborted || evaluated.aborted) break;
    const identity = evaluated.results.find(r => r.index === 0);
    const baseline = identity?.stats[options.stat];
    const limits = identity?.unsupported ?? [];
    const baselineLimits = new Set(limits);
    const ready = batch.map((row, index): PowerRow => {
      const result = evaluated.results.find(r => r.index === index + 1);
      const unsupported = result?.unsupported?.filter(text => !baselineLimits.has(text)) ?? [];
      const finite = Number.isFinite(baseline) && Number.isFinite(result?.stats[options.stat]);
      const status = identity?.error || result?.error || !finite ? 'error' : unsupported.length ? 'unsupported' : 'ready';
      const gain = status === 'ready' ? result!.stats[options.stat] - baseline! : null;
      return { ...row, status, gain, perPoint: gain === null ? null : gain / row.path!.length,
        diagnostics: [identity?.error, result?.error, ...unsupported].filter((v): v is string => Boolean(v)) };
    });
    // Keep the single-node column comparable to the existing heatmap. It is not
    // the route's gain and never supplies the per-point score.
    if (options.signal?.aborted) break;
    const heat = await probe(options.request, options.stat, 0, ready.filter(r => r.status === 'ready').map(r => r.node.skill));
    if (options.signal?.aborted) break;
    const singles = new Map(heat.entries.map(e => [e.skill, e.delta]));
    for (const row of ready) {
      const value = singles.get(row.node.skill);
      row.single = Number.isFinite(value) ? value! : null;
    }
    results.push(...ready);
    options.onBatch?.(ready, results.length, pending.length, limits);
    await new Promise(resolve => setTimeout(resolve, 0));
  }
  return { rows: results, cancelled: Boolean(options.signal?.aborted) };
}
