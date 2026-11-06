import { beforeEach, expect, it, vi } from 'vitest';
let useGemDescriptions: typeof import('./useGemDescriptions')['useGemDescriptions'];
import type { GemDescriptionIndex } from '../lib/gemDescriptions';

const ui = vi.hoisted(() => ({ value: undefined as GemDescriptionIndex | null | undefined,
  effect: null as (() => (() => void) | void) | null, load: vi.fn(), set: vi.fn() }));
vi.mock('react', () => ({
  useState: () => [ui.value, (value: typeof ui.value) => { ui.value = value; ui.set(value); }],
  useEffect: (effect: typeof ui.effect) => { ui.effect = effect; },
}));
vi.mock('../api/backend', () => ({ getBackend: async () => ({ gemDescriptions: ui.load }) }));
beforeEach(async () => {
  vi.resetModules();
  ({ useGemDescriptions } = await import('./useGemDescriptions'));
  ui.value = undefined; ui.effect = null; ui.load.mockReset(); ui.set.mockClear();
});
const flush = async () => { await new Promise(resolve => setTimeout(resolve, 0)); };

it.each([null, { scopes: {}, variants: {} } as GemDescriptionIndex])('returns undefined while loading, then the backend result %j', async value => {
  let resolve!: (value: typeof ui.value) => void;
  ui.load.mockReturnValue(new Promise(done => { resolve = done; }));
  expect(useGemDescriptions(true)).toBeUndefined();
  const cleanup = ui.effect!();
  await flush();
  expect(useGemDescriptions(true)).toBeUndefined();
  resolve(value); await flush();
  expect(useGemDescriptions(true)).toBe(value);
  cleanup?.();
});

it('load errors display as null and log once across consumers', async () => {
  const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
  ui.load.mockRejectedValue(new Error('bad pack'));
  try {
    useGemDescriptions(true); ui.effect!(); await flush();
    expect(useGemDescriptions(true)).toBeNull();
    ui.value = undefined;
    useGemDescriptions(true); ui.effect!(); await flush();
    expect(useGemDescriptions(true)).toBeNull();
    expect(warn).toHaveBeenCalledOnce();
  } finally { warn.mockRestore(); }
});

it('does not publish a late result after unmount', async () => {
  let resolve!: (value: null) => void;
  ui.load.mockReturnValue(new Promise(done => { resolve = done; }));
  useGemDescriptions(true);
  const cleanup = ui.effect!();
  await flush(); cleanup?.(); resolve(null); await flush();
  expect(ui.set).not.toHaveBeenCalled();
});


it('shares one in-flight and resolved promise across consumers, without another Worker RPC on later mounts', async () => {
  const data = { scopes: {}, variants: {} } as GemDescriptionIndex;
  let resolve!: (value: GemDescriptionIndex) => void;
  ui.load.mockReturnValue(new Promise(done => { resolve = done; }));
  for (let i = 0; i < 3; i++) { useGemDescriptions(true); ui.effect!(); }
  await flush();
  expect(ui.load).toHaveBeenCalledOnce();
  resolve(data); await flush();
  expect(ui.set).toHaveBeenCalledTimes(3);
  expect(ui.set.mock.calls.every(([value]) => value === data)).toBe(true);
  ui.value = undefined;
  useGemDescriptions(true); ui.effect!(); await flush();
  expect(ui.load).toHaveBeenCalledOnce();
  expect(ui.value).toBe(data);
});

it('resets a failed shared promise so a later mount retries', async () => {
  const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
  ui.load.mockRejectedValueOnce(new Error('temporary')).mockResolvedValueOnce(null);
  try {
    useGemDescriptions(true); ui.effect!(); await flush();
    expect(ui.value).toBeNull();
    ui.value = undefined;
    useGemDescriptions(true); ui.effect!(); await flush();
    expect(ui.load).toHaveBeenCalledTimes(2);
    expect(ui.value).toBeNull();
  } finally { warn.mockRestore(); }
});

it('loads only after opening and retains the loaded index while closed and on reopening', async () => {
  const data = { scopes: {}, variants: {} } as GemDescriptionIndex;
  ui.load.mockResolvedValue(data);
  expect(useGemDescriptions(false)).toBeUndefined(); ui.effect!(); await flush();
  expect(ui.load).not.toHaveBeenCalled();
  useGemDescriptions(true); const cleanup = ui.effect!(); await flush();
  expect(ui.value).toBe(data);
  cleanup?.();
  expect(useGemDescriptions(false)).toBe(data); ui.effect!(); await flush();
  useGemDescriptions(true); ui.effect!(); await flush();
  expect(ui.value).toBe(data);
  expect(ui.load).toHaveBeenCalledOnce();
});
