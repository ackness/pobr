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

it('leaves library navigation alone and consumes explicit import focus across remounts and revisits', () => {
  expect(renderShell('build')()).toBe(false);
  expect(renderShell('overview', 1)()).toBe(false);
  expect(renderShell('build', 1)()).toBe(true);
  expect(renderShell('build', 1)()).toBe(false);
  expect(renderShell('skills', 1)()).toBe(false);
  expect(renderShell('build', 1)()).toBe(false);
  expect(renderShell('build', 2)()).toBe(true);
});
