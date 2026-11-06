import { afterEach, expect, it, vi } from 'vitest';
import { withViewTransition } from './viewTransition';

vi.mock('react-dom', () => ({ flushSync: (update: () => void) => update() }));
afterEach(() => { vi.unstubAllGlobals(); });

it('keeps both navigation updates and handles an animation skipped by a newer navigation', async () => {
  const report = vi.fn();
  vi.stubGlobal('reportError', report);
  vi.stubGlobal('matchMedia', () => ({ matches: false }));
  let skip!: (reason: unknown) => void;
  const first = new Promise<void>((_, reject) => { skip = reject; });
  const ready = [first, Promise.resolve()];
  vi.stubGlobal('document', { startViewTransition: (update: () => void) => {
    update();
    return { ready: ready.shift()! };
  } });
  const visits: string[] = [];
  withViewTransition(() => visits.push('build'));
  withViewTransition(() => visits.push('skills'));
  skip(new DOMException('A newer transition started', 'AbortError'));
  await Promise.resolve();
  expect(visits).toEqual(['build', 'skills']);
  expect(report).not.toHaveBeenCalled();
});

it('reports unexpected animation errors', async () => {
  const report = vi.fn();
  vi.stubGlobal('reportError', report);
  vi.stubGlobal('matchMedia', () => ({ matches: false }));
  const error = new Error('Failed to capture page');
  vi.stubGlobal('document', { startViewTransition: (update: () => void) => {
    update();
    return { ready: Promise.reject(error) };
  } });
  const update = vi.fn();
  withViewTransition(update);
  await Promise.resolve();
  expect(update).toHaveBeenCalledOnce();
  expect(report).toHaveBeenCalledWith(error);
});

it('updates immediately without an animation when reduced motion is requested', () => {
  vi.stubGlobal('matchMedia', () => ({ matches: true }));
  const start = vi.fn();
  vi.stubGlobal('document', { startViewTransition: start });
  const update = vi.fn();
  withViewTransition(update);
  expect(update).toHaveBeenCalledOnce();
  expect(start).not.toHaveBeenCalled();
});
