import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { BuildSession } from '../../../hooks/useBuildSession';
import { useSpiritReservations } from './useSpiritReservations';
const h = vi.hoisted(() => ({ snapshot: undefined as unknown, deps: undefined as unknown[] | undefined,
  effects: [] as (() => (() => void) | void)[], cleanup: undefined as (() => void) | undefined, calculate: vi.fn() }));
vi.mock('react', () => ({
  useState: () => [h.snapshot, (value: unknown) => { h.snapshot = value; }],
  useEffect: (effect: () => (() => void) | void, deps: unknown[]) => {
    if (!h.deps || deps.some((dep, i) => dep !== h.deps![i])) { h.effects.push(effect); h.deps = deps; }
  },
}));
vi.mock('../../../api/backend', () => ({ getBackend: async () => ({ calculateBuild: h.calculate }) }));
const session = { stateVersion: 1, busy: false, currentRequest: () => ({ socket_groups: [] }) } as unknown as BuildSession;
function render(refresh = 0, active = true) {
  const result = useSpiritReservations(session, active, refresh);
  h.effects.splice(0).forEach(effect => { h.cleanup?.(); h.cleanup = effect() || undefined; });
  return result;
}
beforeEach(() => { h.snapshot = undefined; h.deps = undefined; h.effects = []; h.cleanup = undefined; h.calculate.mockReset(); vi.useFakeTimers(); });
afterEach(() => { h.cleanup?.(); vi.useRealTimers(); });
it('retries a failed Spirit inspection without editing the build and cancels on leaving Skills', async () => {
  h.calculate.mockRejectedValueOnce(new Error('unavailable'));
  render(); await vi.advanceTimersByTimeAsync(250);
  expect(render()).toBeNull();
  h.calculate.mockResolvedValueOnce({ stats: [{ id: 'SpiritReserved', value: 0 }] });
  render(1); await vi.advanceTimersByTimeAsync(250);
  expect(render(1)).toEqual({ groups: new Map(), shared: [], limited: false });
  expect(h.calculate).toHaveBeenCalledTimes(2);
  render(2); render(2, false); await vi.advanceTimersByTimeAsync(250);
  expect(h.calculate).toHaveBeenCalledTimes(2);
});
