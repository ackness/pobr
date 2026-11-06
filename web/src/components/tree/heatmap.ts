import type { PobrBackend } from '../../api/backend';
import type { CalculateBuildRequest } from '../../api/types';

/** Yield between bounded Worker calls so cancellation never queues the rest of the tree. */
export async function evaluateHeatmap({ request, stat, depth, signal, probe, onBatch }: {
  request: CalculateBuildRequest;
  stat: string;
  depth: number;
  signal: AbortSignal;
  probe: PobrBackend['nodePower'];
  onBatch: (values: Map<number, number>, done: number, total: number) => void;
}): Promise<boolean> {
  const step = 64;
  const values = new Map<number, number>();
  for (let offset = 0; !signal.aborted; offset += step) {
    const result = await probe(request, stat, depth, undefined, { offset, limit: step });
    if (signal.aborted) return false;
    result.entries.forEach(entry => values.set(entry.skill, entry.delta));
    const total = result.total ?? result.entries.length;
    const done = result.total === undefined ? total : Math.min(offset + step, total);
    onBatch(new Map(values), done, total);
    if (done >= total) return true;
    await new Promise(resolve => setTimeout(resolve, 0));
  }
  return false;
}
