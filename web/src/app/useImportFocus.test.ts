import { beforeEach, expect, it, vi } from 'vitest';
import type { Route } from './routes';
import { useImportFocus } from './useImportFocus';

const harness = vi.hoisted(() => ({ index: 0, values: [] as { value: unknown; deps?: unknown[] }[] }));
vi.mock('react', () => {
  const memo = (factory: () => unknown, deps: unknown[]) => {
    const index = harness.index++, previous = harness.values[index];
    if (!previous || deps.some((value, at) => value !== previous.deps?.[at])) harness.values[index] = { value: factory(), deps };
    return harness.values[index].value;
  };
  return { useMemo: memo, useCallback: (callback: unknown, deps: unknown[]) => memo(() => callback, deps),
    useRef: (initial: unknown) => (harness.values[harness.index++] ??= { value: { current: initial } }).value };
});
const renderShell = (route: Route, request = 0) => { harness.index = 0; return useImportFocus(route, request); };
beforeEach(() => { harness.index = 0; harness.values = []; });

it('consumes focus once per route entry, retaining consumption when the stage-keyed child remounts', () => {
  const firstPage = renderShell('build');
  expect(firstPage()).toBe(true);
  // The shell survives a build/stage switch and hands the newly mounted page the same callback.
  const remountedPage = renderShell('build');
  expect(remountedPage).toBe(firstPage);
  expect(remountedPage()).toBe(false);
  expect(renderShell('skills')()).toBe(false);
  expect(renderShell('build')()).toBe(true);
  expect(renderShell('build')()).toBe(false);
});

it('allows an explicit topbar request to focus again while already on the library route', () => {
  expect(renderShell('build')()).toBe(true);
  expect(renderShell('build', 1)()).toBe(true);
  expect(renderShell('build', 1)()).toBe(false);
});
