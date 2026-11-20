/**
 * Optimization framework (frontend half): Rust `optimize_variants_json` computes `variant -> stat values`;
 * this module handles the cheap side: scoring objectives, constraints, sorting, subset enumeration, and batching.
 *
 * Performance contract:
 * - Always collect the full `COLLECT_STATS` set during evaluation (each recalc already produces all display stats,
 *   so fetching extra fields has zero marginal cost) -> switching objectives/constraints re-ranks in JS without recalc;
 * - Because wasm executes synchronously on a single thread, evaluate variants in small batches and yield to the
 *   main thread between batches (progress callback + AbortSignal cancellation).
 */

import { getBackend } from '../api/backend';
import type {
  CalculateBuildRequest,
  StatConstraint,
  VariantInput,
  VariantStats,
} from '../api/types';

/** Scoring objective: maximize `stat` (or `stat / per`), subject to hard constraints. */
export interface Objective {
  stat: string;
  /** Ratio denominator (e.g., `ManaCost` -> DPS per mana cost efficiency). */
  per?: string;
  /** Geometric mean gives equal importance to relative gains in both stats. */
  secondaryStat?: string;
  /** Soft minimums are prioritized after hard constraints, before score. */
  softMinimums?: { stat: string; min: number }[];
  constraints: StatConstraint[];
}

/** Objective presets (i18n keys resolved at the UI layer). */
export const OBJECTIVE_PRESETS: { id: string; stat: string; per?: string; secondaryStat?: string; labelKey: string }[] = [
  { id: 'balanced', stat: 'TotalDPS', secondaryStat: 'TotalEHP', labelKey: 'trade.balanced' },
  { id: 'dps', stat: 'TotalDPS', labelKey: 'opt.objDps' },
  { id: 'dpsPerMana', stat: 'TotalDPS', per: 'ManaCost', labelKey: 'opt.objDpsPerMana' },
  { id: 'life', stat: 'Life', labelKey: 'opt.objLife' },
  { id: 'ehp', stat: 'TotalEHP', labelKey: 'opt.objEhp' },
];

/** Available constraint stats (display_catalog field IDs; labels reuse the sidebar dictionary). */
export const CONSTRAINT_STATS = ['ActionRate', 'ManaCost', 'Life', 'TotalDPS', 'TotalEHP', 'CritChance'];

/**
 * Full stat set always collected during evaluation = all preset objectives/denominators + all constraint candidates.
 * Allows switching objectives afterward with zero recalculation.
 */
export const COLLECT_STATS: string[] = [
  ...new Set([
    ...OBJECTIVE_PRESETS.flatMap((p) => (p.per ? [p.stat, p.per] : [p.stat])),
    ...CONSTRAINT_STATS,
    'CombinedDPS',
    'MainMinionDPS',
    'FireResist', 'ColdResist', 'LightningResist', 'ChaosResist', 'ProjectileCount', 'AoeRadius',
  ]),
];

/** Matches the Rust-side VARIANT_CAP total limit (applies across batches to prevent accidental 10k+ enumerations). */
export const VARIANT_TOTAL_CAP = 512;

/** Variants per batch: yields to the main thread between batches so the UI stays responsive. */
const BATCH_SIZE = 16;

/** Objective score: when ratio denominator <= 0, falls back to raw target value (e.g., 0-cost skills). */
export function scoreOf(stats: Record<string, number>, objective: Objective): number {
  const obj = stats[objective.stat] ?? 0;
  if (objective.secondaryStat) return Math.sqrt(Math.max(0, obj)) * Math.sqrt(Math.max(0, stats[objective.secondaryStat] ?? 0));
  if (!objective.per) return obj;
  const per = stats[objective.per] ?? 0;
  return per > 0 ? obj / per : obj;
}

export function minimumDeficit(stats: Record<string, number>, objective: Objective): number {
  return (objective.softMinimums ?? []).reduce((sum, target) => sum + Math.max(0, target.min - (stats[target.stat] ?? 0)), 0);
}

/** Negative means a is preferred; all calculations remain relative to one build. */
export function compareObjectiveStats(a: Record<string, number>, b: Record<string, number>, objective: Objective): number {
  const feasible = Number(feasibleOf(b, objective)) - Number(feasibleOf(a, objective));
  return feasible || minimumDeficit(a, objective) - minimumDeficit(b, objective) || scoreOf(b, objective) - scoreOf(a, objective);
}

/** Whether all hard constraints are satisfied. */
export function feasibleOf(stats: Record<string, number>, objective: Objective): boolean {
  return objective.constraints.every((c) => {
    const v = stats[c.stat] ?? 0;
    return (c.min === undefined || v >= c.min) && (c.max === undefined || v <= c.max);
  });
}

/** Shallow copy sorted by (feasible first, highest score first); errored variants sort last. */
export function rankVariants(results: VariantStats[], objective: Objective): VariantStats[] {
  return [...results].sort((a, b) => {
    if (Boolean(a.error) !== Boolean(b.error)) return a.error ? 1 : -1;
    return compareObjectiveStats(a.stats, b.stats, objective);
  });
}

export interface EvaluateOptions {
  /** Explicit display stats for reports; ordinary planners retain their shared set. */
  stats?: string[];
  request: CalculateBuildRequest;
  variants: VariantInput[];
  onProgress?: (done: number, total: number) => void;
  signal?: AbortSignal;
}

export interface EvaluateResult {
  baseline: Record<string, number>;
  results: VariantStats[];
  /** `true` when aborted by `signal`; `results` contains completed entries so far. */
  aborted: boolean;
}

/**
 * Evaluate variants in batches: first batch includes baseline, yielding to the main thread
 * between batches (progress rendering + cancellation responsiveness).
 * Cancellation does not throw—returns whatever finished so usable partial results still render.
 */
export async function evaluateVariants(opts: EvaluateOptions): Promise<EvaluateResult> {
  const { request, variants, onProgress, signal } = opts;
  if (variants.length > VARIANT_TOTAL_CAP) {
    throw new Error(`${variants.length} variants exceed cap ${VARIANT_TOTAL_CAP}`);
  }
  const backend = await getBackend();
  let baseline: Record<string, number> = {};
  const results: VariantStats[] = [];
  for (let start = 0; start < variants.length; start += BATCH_SIZE) {
    if (signal?.aborted) return { baseline, results, aborted: true };
    const batch = variants.slice(start, start + BATCH_SIZE);
    const resp = await backend.optimizeVariants({
      request,
      stats: opts.stats ?? COLLECT_STATS,
      variants: batch,
      include_baseline: start === 0,
    });
    if (resp.baseline) baseline = resp.baseline;
    // `v.index` is batch-local -> map back to global index.
    results.push(...resp.variants.map((v) => ({ ...v, index: start + v.index })));
    onProgress?.(Math.min(start + batch.length, variants.length), variants.length);
    // Yield to the main thread so progress updates paint and cancel clicks dispatch.
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  return { baseline, results, aborted: false };
}
