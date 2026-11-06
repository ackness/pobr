import { afterEach, expect, test, vi } from 'vitest';
import { fitToParentInPixels } from './pixelSize';

type Callback = (entries: Partial<ResizeObserverEntry>[]) => void;
const observers: { callback: Callback; target: unknown; disconnected: boolean }[] = [];

class FakeResizeObserver {
  record: (typeof observers)[number];
  constructor(callback: Callback) { this.record = { callback, target: null, disconnected: false }; observers.push(this.record); }
  observe(target: unknown) { this.record.target = target; }
  disconnect() { this.record.disconnected = true; }
}

function fakeElement(parent: object | null) {
  const values: Record<string, string> = { width: '', height: '' };
  const style = Object.assign(values, { removeProperty(name: string) { values[name] = ''; } });
  return { parentElement: parent, style } as unknown as SVGElement & { style: Record<string, string> };
}

afterEach(() => { observers.length = 0; vi.unstubAllGlobals(); });

test('follows the parent content box in pixels and hands the size back to CSS when stopped', () => {
  vi.stubGlobal('ResizeObserver', FakeResizeObserver);
  const parent = {};
  const svg = fakeElement(parent);
  const stop = fitToParentInPixels(svg);
  expect(observers[0].target).toBe(parent);
  // Until the first measurement the stylesheet's percentage size applies.
  expect(svg.style.width).toBe('');

  observers[0].callback([{ contentBoxSize: [{ inlineSize: 728.5, blockSize: 714 }] }]);
  expect([svg.style.width, svg.style.height]).toEqual(['728.5px', '714px']);
  observers[0].callback([{ contentRect: { width: 390, height: 300 } as DOMRectReadOnly }]);
  expect([svg.style.width, svg.style.height]).toEqual(['390px', '300px']);

  stop();
  expect(observers[0].disconnected).toBe(true);
  expect([svg.style.width, svg.style.height]).toEqual(['', '']);
});

test('does nothing without ResizeObserver or a parent', () => {
  vi.stubGlobal('ResizeObserver', undefined);
  fitToParentInPixels(fakeElement({}))();
  vi.stubGlobal('ResizeObserver', FakeResizeObserver);
  fitToParentInPixels(fakeElement(null))();
  expect(observers).toHaveLength(0);
});
