/**
 * 寻优框架（前端半边）：Rust 侧 `optimize_variants_json` 只算「变体 → 属性值」，
 * 这里负责便宜的部分——目标打分 / 约束判定 / 排序 / 子集枚举 / 分批评估。
 *
 * 效率契约：
 * - 评估时固定收集 `COLLECT_STATS` 全集（引擎一次重算本就产出全部展示属性，
 *   多取几个字段零成本）→ 切换目标/约束只在前端重排，不再重算；
 * - wasm 单线程同步执行，变体按小批多次调用，批间让出主线程（进度回调 +
 *   AbortSignal 取消）。
 */

import { getBackend } from '../api/backend';
import type {
  CalculateBuildRequest,
  StatConstraint,
  VariantInput,
  VariantStats,
} from '../api/types';

/** 打分目标：最大化 stat（或 stat/per 比值），可叠硬约束。 */
export interface Objective {
  stat: string;
  /** 比值分母（如 ManaCost → DPS÷魔耗效率）。 */
  per?: string;
  constraints: StatConstraint[];
}

/** 目标预设（i18n key 由 UI 层解释）。 */
export const OBJECTIVE_PRESETS: { id: string; stat: string; per?: string; labelKey: string }[] = [
  { id: 'dps', stat: 'TotalDPS', labelKey: 'opt.objDps' },
  { id: 'dpsPerMana', stat: 'TotalDPS', per: 'ManaCost', labelKey: 'opt.objDpsPerMana' },
  { id: 'life', stat: 'Life', labelKey: 'opt.objLife' },
  { id: 'ehp', stat: 'TotalEHP', labelKey: 'opt.objEhp' },
];

/** 约束可选属性（display_catalog 字段 id；标签复用侧边栏字典）。 */
export const CONSTRAINT_STATS = ['ActionRate', 'ManaCost', 'Life', 'TotalDPS', 'CritChance'];

/**
 * 评估时固定收集的属性全集 = 全部预设目标/分母 + 全部约束候选。
 * 一次评估后任意切换目标零重算。
 */
export const COLLECT_STATS: string[] = [
  ...new Set([
    ...OBJECTIVE_PRESETS.flatMap((p) => (p.per ? [p.stat, p.per] : [p.stat])),
    ...CONSTRAINT_STATS,
  ]),
];

/** 与 Rust 侧 VARIANT_CAP 一致的总量上限（分批照样受限，防手滑万级枚举）。 */
export const VARIANT_TOTAL_CAP = 512;

/** 单批变体数：批间让出主线程，UI 保持响应。 */
const BATCH_SIZE = 16;

/** 目标得分：比值分母 ≤ 0 时退化为纯目标值（如 0 蓝耗技能）。 */
export function scoreOf(stats: Record<string, number>, objective: Objective): number {
  const obj = stats[objective.stat] ?? 0;
  if (!objective.per) return obj;
  const per = stats[objective.per] ?? 0;
  return per > 0 ? obj / per : obj;
}

/** 全部约束满足？ */
export function feasibleOf(stats: Record<string, number>, objective: Objective): boolean {
  return objective.constraints.every((c) => {
    const v = stats[c.stat] ?? 0;
    return (c.min === undefined || v >= c.min) && (c.max === undefined || v <= c.max);
  });
}

/** 按（可行优先，得分降序）排序的浅拷贝；带 error 的变体沉底。 */
export function rankVariants(results: VariantStats[], objective: Objective): VariantStats[] {
  return [...results].sort((a, b) => {
    if (Boolean(a.error) !== Boolean(b.error)) return a.error ? 1 : -1;
    const fa = feasibleOf(a.stats, objective);
    const fb = feasibleOf(b.stats, objective);
    if (fa !== fb) return fa ? -1 : 1;
    return scoreOf(b.stats, objective) - scoreOf(a.stats, objective);
  });
}

/** 非空子集（大小 ≤ maxSize），生成顺序：先小组合后大组合。 */
export function subsets<T>(items: T[], maxSize: number): T[][] {
  const out: T[][] = [];
  const n = items.length;
  const cap = Math.min(maxSize, n);
  for (let mask = 1; mask < 1 << n; mask += 1) {
    const picked = items.filter((_, i) => mask & (1 << i));
    if (picked.length <= cap) out.push(picked);
  }
  return out.sort((a, b) => a.length - b.length);
}

/** 非空子集数 Σ C(n,k), k ≤ cap（枚举前的组合数预检）。 */
export function subsetCount(n: number, cap: number): number {
  let total = 0;
  let c = 1;
  for (let k = 1; k <= Math.min(n, cap); k += 1) {
    c = (c * (n - k + 1)) / k;
    total += c;
  }
  return total;
}

export interface EvaluateOptions {
  request: CalculateBuildRequest;
  variants: VariantInput[];
  onProgress?: (done: number, total: number) => void;
  signal?: AbortSignal;
}

export interface EvaluateResult {
  baseline: Record<string, number>;
  results: VariantStats[];
  /** true = 被 signal 中止，results 为已完成的部分。 */
  aborted: boolean;
}

/**
 * 分批评估变体：首批带基线，批间让出主线程（进度渲染 + 取消响应）。
 * 中止不抛错——返回已算完的部分（可用则照常展示）。
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
      stats: COLLECT_STATS,
      variants: batch,
      include_baseline: start === 0,
    });
    if (resp.baseline) baseline = resp.baseline;
    // index 是批内下标 → 折回全局下标。
    results.push(...resp.variants.map((v) => ({ ...v, index: start + v.index })));
    onProgress?.(Math.min(start + batch.length, variants.length), variants.length);
    // 让出主线程：进度条得以绘制、取消点击得以派发。
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  return { baseline, results, aborted: false };
}
