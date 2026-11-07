import { afterEach, expect, test, vi } from 'vitest';
import { BAR_BOTTOM_VAR, trackBarBottom } from './barClearance';

const observers: { callback: () => void; target: unknown; disconnected: boolean }[] = [];
class FakeResizeObserver {
  record: (typeof observers)[number];
  constructor(callback: () => void) { this.record = { callback, target: null, disconnected: false }; observers.push(this.record); }
  observe(target: unknown) { this.record.target = target; }
  disconnect() { this.record.disconnected = true; }
}

function fakeBar(parent: object | null, offsetTop: number) {
  return { parentElement: parent, offsetTop, offsetHeight: 40 } as unknown as HTMLElement & { offsetHeight: number };
}
function fakeParent() {
  const values: Record<string, string> = {};
  const style = {
    setProperty(name: string, value: string) { values[name] = value; },
    removeProperty(name: string) { delete values[name]; },
  };
  return { style, values };
}

afterEach(() => { observers.length = 0; vi.unstubAllGlobals(); });

test('publishes where the bar ends on every resize and clears it when stopped', () => {
  vi.stubGlobal('ResizeObserver', FakeResizeObserver);
  const parent = fakeParent();
  const bar = fakeBar(parent, 12);
  const { values } = parent;
  const stop = trackBarBottom(bar);
  expect(observers[0].target).toBe(bar);
  observers[0].callback();
  expect(values[BAR_BOTTOM_VAR]).toBe('52px');
  // The bar wraps to a second row.
  (bar as { offsetHeight: number }).offsetHeight = 83;
  observers[0].callback();
  expect(values[BAR_BOTTOM_VAR]).toBe('95px');

  stop?.();
  expect(observers[0].disconnected).toBe(true);
  expect(values[BAR_BOTTOM_VAR]).toBeUndefined();
});

test('does nothing without ResizeObserver or a parent', () => {
  vi.stubGlobal('ResizeObserver', undefined);
  expect(trackBarBottom(fakeBar(fakeParent(), 0))).toBeUndefined();
  vi.stubGlobal('ResizeObserver', FakeResizeObserver);
  expect(trackBarBottom(fakeBar(null, 0))).toBeUndefined();
  expect(trackBarBottom(null)).toBeUndefined();
  expect(observers).toHaveLength(0);
});
