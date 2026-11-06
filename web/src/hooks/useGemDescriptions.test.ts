import { beforeEach, expect, it, vi } from 'vitest';
import { useGemDescriptions } from './useGemDescriptions';
import type { GemDescriptionIndex } from '../lib/gemDescriptions';

const ui = vi.hoisted(() => ({ value: undefined as GemDescriptionIndex | null | undefined,
  effect: null as (() => (() => void)) | null, load: vi.fn(), set: vi.fn() }));
vi.mock('react', () => ({
  useState: () => [ui.value, (value: typeof ui.value) => { ui.value = value; ui.set(value); }],
  useEffect: (effect: typeof ui.effect) => { ui.effect = effect; },
}));
vi.mock('../api/backend', () => ({ getBackend: async () => ({ gemDescriptions: ui.load }) }));
beforeEach(() => { ui.value = undefined; ui.effect = null; ui.load.mockReset(); ui.set.mockClear(); });
const flush = async () => { await new Promise(resolve => setTimeout(resolve, 0)); };

it.each([null, { scopes: {}, variants: {} } as GemDescriptionIndex])('returns undefined while loading, then the backend result %j', async value => {
  let resolve!: (value: typeof ui.value) => void;
  ui.load.mockReturnValue(new Promise(done => { resolve = done; }));
  expect(useGemDescriptions()).toBeUndefined();
  const cleanup = ui.effect!();
  await flush();
  expect(useGemDescriptions()).toBeUndefined();
  resolve(value); await flush();
  expect(useGemDescriptions()).toBe(value);
  cleanup();
});

it('load errors display as null and log once across consumers', async () => {
  const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
  ui.load.mockRejectedValue(new Error('bad pack'));
  try {
    useGemDescriptions(); ui.effect!(); await flush();
    expect(useGemDescriptions()).toBeNull();
    ui.value = undefined;
    useGemDescriptions(); ui.effect!(); await flush();
    expect(useGemDescriptions()).toBeNull();
    expect(warn).toHaveBeenCalledOnce();
  } finally { warn.mockRestore(); }
});

it('does not publish a late result after unmount', async () => {
  let resolve!: (value: null) => void;
  ui.load.mockReturnValue(new Promise(done => { resolve = done; }));
  useGemDescriptions();
  const cleanup = ui.effect!();
  await flush(); cleanup(); resolve(null); await flush();
  expect(ui.set).not.toHaveBeenCalled();
});
