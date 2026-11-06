import { expect, test, vi } from 'vitest';
import { evaluateHeatmap } from './heatmap';
import type { PobrBackend } from '../../api/backend';

const probe = vi.fn<PobrBackend['nodePower']>(async (_request, _stat, _depth, _ids, window) => ({
  base: 100, total: 150, entries: Array.from({ length: Math.min(window!.limit, 150 - window!.offset) }, (_, index) =>
    ({ skill: window!.offset + index, delta: index - 10, distance: 1 })),
}));

test('heatmap reports total progress across bounded windows and retains each node gain', async () => {
  probe.mockClear();
  const batches: [Map<number, number>, number, number][] = [];
  const complete = await evaluateHeatmap({ request: {}, stat: 'Life', depth: 5, signal: new AbortController().signal, probe,
    onBatch: (values, done, total) => batches.push([values, done, total]) });
  expect(complete).toBe(true);
  expect(batches.map(([, done, total]) => [done, total])).toEqual([[64, 150], [128, 150], [150, 150]]);
  expect(batches[2][0].size).toBe(150);
  expect(batches[0][0].get(0)).toBe(-10);
  expect(probe.mock.calls.map(args => args[4])).toEqual([{ offset: 0, limit: 64 }, { offset: 64, limit: 64 }, { offset: 128, limit: 64 }]);
});

test('cancelled heatmap stops queueing batches and never claims completion', async () => {
  probe.mockClear();
  const controller = new AbortController();
  const onBatch = vi.fn(() => controller.abort());
  expect(await evaluateHeatmap({ request: {}, stat: 'Life', depth: 5, signal: controller.signal, probe, onBatch })).toBe(false);
  expect(probe).toHaveBeenCalledTimes(1);
  expect(onBatch).toHaveBeenCalledOnce();
});

test('cancelling in flight discards the response and an older engine without windows completes once', async () => {
  const controller = new AbortController();
  const onBatch = vi.fn();
  const cancelledProbe: PobrBackend['nodePower'] = async () => { controller.abort(); return { base: 1, total: 150, entries: [] }; };
  expect(await evaluateHeatmap({ request: {}, stat: 'Life', depth: 5, signal: controller.signal, probe: cancelledProbe, onBatch })).toBe(false);
  expect(onBatch).not.toHaveBeenCalled();
  const legacy = vi.fn<PobrBackend['nodePower']>(async () => ({ base: 1, entries: [{ skill: 1, delta: 5, distance: 1 }] }));
  expect(await evaluateHeatmap({ request: {}, stat: 'Life', depth: 5, signal: new AbortController().signal, probe: legacy, onBatch })).toBe(true);
  expect(legacy).toHaveBeenCalledOnce();
  expect(onBatch).toHaveBeenCalledWith(new Map([[1, 5]]), 1, 1);
});
