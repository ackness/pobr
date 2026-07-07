/**
 * 对比预览：把「改一处后的请求」再算一次，与当前结果逐字段求差。
 * PoB2 的装备对比 / 天赋点收益提示都用这一个原语。
 */

import { getBackend } from '../api/backend';
import type { CalculateBuildRequest, CalculateBuildResponse, DisplayStatValue } from '../api/types';

export interface DiffEntry {
  id: string;
  before: number;
  after: number;
  delta: number;
}

/** 两份展示字段的非零差异（忽略 null/∞ 字段与 <0.005 的浮点噪声）。 */
export function diffStats(before: DisplayStatValue[], after: DisplayStatValue[]): DiffEntry[] {
  const beforeMap = new Map(before.map((s) => [s.id, s.value]));
  const out: DiffEntry[] = [];
  for (const stat of after) {
    const prev = beforeMap.get(stat.id);
    if (typeof prev !== 'number' || typeof stat.value !== 'number') continue;
    const delta = stat.value - prev;
    if (Math.abs(delta) < 0.005) continue;
    out.push({ id: stat.id, before: prev, after: stat.value, delta });
  }
  // 大变化在前。
  out.sort((a, b) => Math.abs(b.delta) - Math.abs(a.delta));
  return out;
}

/** 算一次候选请求并与当前结果求差。 */
export async function previewDiff(
  request: CalculateBuildRequest,
  current: CalculateBuildResponse,
): Promise<DiffEntry[]> {
  const backend = await getBackend();
  const next = await backend.calculateBuild(request);
  return diffStats(current.stats, next.stats);
}
