import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { CalculateBuildRequest, CalculateBuildResponse, SkillMetricsResponse } from '../api/types';
import { advanceImportReview } from '../app/importReview';
import { defaultStarterSession } from '../lib/buildWorkspace';
import { useBuildSession } from './useBuildSession';

// Replay the real hook's state and callbacks; boot effects are unnecessary for importing a saved session.
const harness = vi.hoisted(() => ({ state: [] as unknown[], refs: [] as { current: unknown }[], stateIndex: 0, refIndex: 0,
  calculate: vi.fn<(request: CalculateBuildRequest) => Promise<CalculateBuildResponse>>(),
  metrics: vi.fn<() => Promise<SkillMetricsResponse>>(),
}));
vi.mock('react', () => ({
  useState: (initial: unknown) => {
    const index = harness.stateIndex++;
    if (!(index in harness.state)) harness.state[index] = typeof initial === 'function' ? initial() : initial;
    return [harness.state[index], (value: unknown) => {
      harness.state[index] = typeof value === 'function' ? value(harness.state[index]) : value;
    }];
  },
  useRef: (initial: unknown) => harness.refs[harness.refIndex++] ??= { current: initial },
  useCallback: (callback: unknown) => callback,
  useEffect: () => {},
}));
vi.mock('../api/backend', () => ({ getBackend: async () => ({
  calculateBuild: harness.calculate, skillMetrics: harness.metrics,
}) }));

function deferred<T>() {
  let resolve!: (value: T) => void, reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
function renderSession() {
  harness.stateIndex = 0; harness.refIndex = 0;
  return useBuildSession();
}
const result = (skill_id: string, value: number) => ({ main_skill: { skill_id }, stats: [{ id: 'CombinedDPS', value }] }) as CalculateBuildResponse;
const initial = result('FireballPlayer', 100), selected = result('SparkPlayer', 900);
function importWithoutSelection() {
  const saved = defaultStarterSession('Witch');
  saved.state.socketGroups = ['FireballPlayer', 'SparkPlayer'].map(skill_id => ({ enabled: true,
    gems: [{ skill_id, level: 20, quality: 0 }] }));
  renderSession().importSession(JSON.stringify(saved));
}

beforeEach(() => {
  harness.state = []; harness.refs = [];
  harness.calculate.mockReset(); harness.metrics.mockReset();
  const storage = new Map<string, string>();
  vi.stubGlobal('localStorage', { getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value) });
});
afterEach(() => vi.unstubAllGlobals());

it('delayed highest-DPS metrics and their recalculation finish before the import summary freezes', async () => {
  const metrics = deferred<SkillMetricsResponse>(), recalculation = deferred<CalculateBuildResponse>();
  harness.metrics.mockReturnValue(metrics.promise);
  harness.calculate.mockImplementation(async request => request.main_socket_group === 1 ? recalculation.promise : initial);
  importWithoutSelection();
  await vi.waitFor(() => expect(renderSession().calc?.main_skill?.skill_id).toBe('FireballPlayer'));
  // The first calculation is already done, but automatic selection is still waiting for metrics.
  const waiting = renderSession();
  expect(waiting.busy).toBe(true);
  const pending = advanceImportReview({ scope: null }, 'import:stage', waiting)!;
  expect(pending.summary).toBeUndefined();
  metrics.resolve({ item_errors: [], skills: [100, 900].map((combined_dps, group_index) => ({
    group_index, active_skill_index: 1, skill_id: ['FireballPlayer', 'SparkPlayer'][group_index], status: 'calculated',
    unsupported_modifiers: [], error: null,
    metrics: { combined_dps, hit_dps: combined_dps, dot_dps: 0, average_hit: 0, action_rate: 1, mana_cost: 0, life_cost: 0, cooldown: 0 },
  })) });
  await vi.waitFor(() => expect(harness.calculate).toHaveBeenCalledTimes(2));
  expect(renderSession().calcParams.main_socket_group).toBe(1);
  expect(renderSession().busy).toBe(true);
  expect(advanceImportReview(pending, 'import:stage', renderSession())?.summary).toBeUndefined();
  recalculation.resolve(selected);
  await vi.waitFor(() => expect(renderSession().busy).toBe(false));
  const summary = advanceImportReview(pending, 'import:stage', renderSession())!.summary!;
  expect(summary.calc?.main_skill?.skill_id).toBe('SparkPlayer');
  expect(summary.calc?.stats[0].value).toBe(900);
});

it('a rejected metrics request releases busy state so the completed calculation can still be summarised', async () => {
  const metrics = deferred<SkillMetricsResponse>();
  harness.metrics.mockReturnValue(metrics.promise);
  harness.calculate.mockResolvedValue(initial);
  importWithoutSelection();
  await vi.waitFor(() => expect(renderSession().calc).toBe(initial));
  expect(renderSession().busy).toBe(true);
  metrics.reject(new Error('Metrics unavailable'));
  await vi.waitFor(() => expect(renderSession().busy).toBe(false));
  expect(advanceImportReview({ scope: null }, 'import:stage', renderSession())?.summary?.calc).toBe(initial);
});
