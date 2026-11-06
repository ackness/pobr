import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { BuildSession } from '../../hooks/useBuildSession';
import type { CalculateBuildResponse } from '../../api/types';
import type { PreviewApi } from '../../components/shared/PreviewContext';
import { PreviewProvider } from './PreviewProvider';

// Replay the provider's hooks without a browser. Keep effects and refs across renders.
const h = vi.hoisted(() => ({ cursor: 0, slots: [] as any[], pending: [] as (() => void)[], calculate: vi.fn(), mutation: () => {} }));
vi.mock('react', async original => ({ ...await original<typeof import('react')>(),
  useState: (initial: unknown) => { const i = h.cursor++; if (!(i in h.slots)) h.slots[i] = initial;
    return [h.slots[i], (next: unknown) => { h.slots[i] = typeof next === 'function' ? next(h.slots[i]) : next; }]; },
  useRef: (initial: unknown) => { const i = h.cursor++; return h.slots[i] ??= { current: initial }; },
  useCallback: (fn: unknown, deps: unknown[]) => { const i = h.cursor++, old = h.slots[i];
    if (!old || deps.some((dep, j) => dep !== old.deps[j])) h.slots[i] = { deps, fn }; return h.slots[i].fn; },
  useMemo: (fn: () => unknown) => fn(),
  useEffect: (effect: () => (() => void) | void, deps: unknown[]) => {
    const i = h.cursor++, old = h.slots[i];
    if (!old || deps.some((dep, j) => dep !== old.deps[j])) h.pending.push(() => { old?.cleanup?.(); h.slots[i] = { deps, cleanup: effect() }; });
  },
}));
vi.mock('../../api/backend', () => ({ getBackend: async () => ({ calculateBuild: h.calculate }) }));
const session = { stateVersion: 1, busy: false } as BuildSession;
const view = { calc: { stats: [{ id: 'Life', value: 100 }] } as CalculateBuildResponse, previous: null, stale: false };
const changes = [{ id: 'Life', before: 100, after: 200, delta: 100, percent: 100, tone: 'better' as const }];
function render(route = 'items', version = 1) {
  h.cursor = 0;
  const element = PreviewProvider({ session: { ...session, stateVersion: version }, view, route, children: null });
  h.pending.splice(0).forEach(fn => fn());
  return element.props.value as PreviewApi;
}
beforeEach(() => {
  h.cursor = 0; h.slots = []; h.pending = []; h.calculate.mockReset(); vi.useFakeTimers();
  vi.stubGlobal('window', globalThis);
  vi.stubGlobal('document', { body: {} });
  vi.stubGlobal('MutationObserver', class { constructor(callback: () => void) { h.mutation = callback; } observe() {} disconnect() {} });
});
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

it('clears the preview when navigating without changing the build version (U123 reproduction)', () => {
  render().showChanges('Library item', changes);
  expect(render().state?.label).toBe('Library item');
  render('overview');
  expect(render('overview').state).toBeNull();
});

it('cancels both hover and hide timers when the baseline changes', async () => {
  const preview = render();
  preview.show({ key: 'item', label: 'Item', request: () => ({}) });
  render('items', 2);
  await vi.advanceTimersByTimeAsync(200);
  expect(h.calculate).not.toHaveBeenCalled();
  expect(render('items', 2).state).toBeNull();
});

it('clears a removed target and ignores its late result', async () => {
  let resolve!: (result: CalculateBuildResponse) => void;
  h.calculate.mockImplementation(() => new Promise(done => { resolve = done; }));
  const target = { isConnected: true, closest: () => null } as unknown as HTMLElement;
  render().show({ key: 'item', label: 'Item', request: () => ({}) }, target);
  await vi.advanceTimersByTimeAsync(120);
  expect(render().state?.status).toBe('loading');
  Object.assign(target, { isConnected: false }); h.mutation();
  resolve({ stats: [{ id: 'Life', value: 200 }] } as CalculateBuildResponse);
  await vi.advanceTimersByTimeAsync(0);
  expect(render().state).toBeNull();
});

it('clears on pointer leave and discards the pending hover request', async () => {
  const preview = render();
  preview.show({ key: 'item', label: 'Item', request: () => ({}) });
  preview.clear();
  await vi.advanceTimersByTimeAsync(250);
  expect(h.calculate).not.toHaveBeenCalled();
  expect(render().state).toBeNull();
});

it('clears known changes when their retained panel becomes hidden', () => {
  let hidden = false;
  const target = { isConnected: true, closest: () => hidden ? {} : null } as unknown as Element;
  render().showChanges('Known upgrade', changes, target);
  expect(render().state?.label).toBe('Known upgrade');
  hidden = true; h.mutation();
  expect(render().state).toBeNull();
});
