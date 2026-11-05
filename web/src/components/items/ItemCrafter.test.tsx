import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, it, vi } from 'vitest';
import type { BuildSession } from '../../hooks/useBuildSession';
import { EquippedItemCrafter, useEquippedItemDraft } from './ItemCrafter';

const harness = vi.hoisted(() => ({ states: [] as unknown[], cursor: 0, effect: undefined as (() => (() => void)) | undefined,
  normalize: vi.fn(), workbench: vi.fn(),
}));
vi.mock('react', async original => ({ ...await original<typeof import('react')>(),
  useState: (initial: unknown) => {
    const index = harness.cursor++;
    if (!(index in harness.states)) harness.states[index] = initial;
    return [harness.states[index], (next: unknown) => { harness.states[index] = next; }];
  },
  useEffect: (effect: () => (() => void)) => { harness.effect = effect; },
}));
vi.mock('../../api/backend', () => ({ getBackend: async () => ({
  normalizeItemDraft: harness.normalize,
  itemCraftingCatalog: async () => ({ bases: [
    { id: 'short', name: 'Ring', item_class: 'Ring', tags: ['ring'], mod_domain: 1 },
    { id: 'ring', name: 'Sapphire Ring', item_class: 'Ring', tags: ['ring'], mod_domain: 1 },
  ], mods: [] }),
}) }));
vi.mock('../../lib/tradeOptimizer', () => ({ loadTradeCatalog: async () => ({ bases: [{ name: 'Ring' }, { name: 'Sapphire Ring' }], mods: [] }) }));
vi.mock('./ItemDraftWorkbench', () => ({ ItemDraftWorkbench: (props: unknown) => { harness.workbench(props); return <div>Draft preview</div>; } }));

const original = 'Rarity: RARE\nVictory Ring\nSapphire Ring\nImplicits: 1\n+50 to maximum Life';
const session = {} as BuildSession;
function Probe({ text }: { text: string }) {
  const { draft, error } = useEquippedItemDraft(text);
  if (error) return <p role="alert">Failed</p>;
  if (!draft) return <p role="status">Loading</p>;
  return <EquippedItemCrafter draft={draft} session={session} lang="en-US" />;
}
const render = (text = original) => { harness.cursor = 0; return renderToStaticMarkup(<Probe text={text} />); };
beforeEach(() => {
  harness.states = [];
  harness.workbench.mockClear();
  // Match normalize_item_draft_json's contract: the parsed header must be an exact base.
  harness.normalize.mockReset().mockImplementation(async (text: string) => {
    const lines = text.split('\n').map(line => line.trim()).filter(Boolean);
    const rarity = lines[0].slice('Rarity: '.length);
    const base = lines[rarity === 'NORMAL' || rarity === 'MAGIC' ? 1 : 2];
    if (!['Ring', 'Sapphire Ring'].includes(base)) throw new Error('invalid_item');
    return text;
  });
});

it('loads the rare base and preserves the equipped item in the draft preview', async () => {
  expect(render()).toContain('role="status"');
  harness.effect!();
  await vi.waitFor(() => expect(harness.states[0]).not.toBeNull());
  expect(render()).toContain('Draft preview');
  expect(harness.normalize).toHaveBeenCalledWith(original);
  expect(harness.workbench).toHaveBeenLastCalledWith(expect.objectContaining({ initialText: original,
    base: expect.objectContaining({ name: 'Sapphire Ring' }), session }));
});

it.each([
  ['NORMAL', 'Sapphire Ring'],
  ['MAGIC', 'Sapphire Ring'],
  ['MAGIC', 'Healthy Sapphire Ring'],
  ['MAGIC', 'Sapphire Ring of the Fox'],
  ['MAGIC', 'Healthy Sapphire Ring of the Fox'],
])('resolves %s header %s before calling the strict normalizer', async (rarity, name) => {
  const tail = '\nItem Level: 50\nImplicits: 1\n+30% to Cold Resistance\n+50 to maximum Life';
  const text = `Rarity: ${rarity}\n${name}${tail}`;
  const canonical = `Rarity: ${rarity}\nSapphire Ring${tail}`;
  if (name !== 'Sapphire Ring') await expect(harness.normalize(text)).rejects.toThrow('invalid_item');
  harness.normalize.mockClear();
  render(text); harness.effect!();
  await vi.waitFor(() => expect(harness.states[0]).not.toBeNull());
  expect(render(text)).toContain('Draft preview');
  expect(harness.normalize).toHaveBeenCalledExactlyOnceWith(canonical);
  expect(harness.workbench).toHaveBeenLastCalledWith(expect.objectContaining({ initialText: canonical,
    base: expect.objectContaining({ name: 'Sapphire Ring' }) }));
});

it('uses the unique base header rather than a title matching another base', async () => {
  const text = 'Rarity: UNIQUE\nRing\nSapphire Ring\nImplicits: 0';
  render(text); harness.effect!();
  await vi.waitFor(() => expect(harness.states[0]).not.toBeNull());
  render(text);
  expect(harness.normalize).toHaveBeenCalledExactlyOnceWith(text);
  expect(harness.workbench).toHaveBeenLastCalledWith(expect.objectContaining({ initialText: text,
    base: expect.objectContaining({ name: 'Sapphire Ring' }) }));
});

it.each(['NORMAL', 'MAGIC', 'RARE'])('does not infer a %s base from modifier text or partial words', async rarity => {
  const name = rarity === 'RARE' ? 'Sapphire Ring' : 'Healthy Sapphire Ringlet';
  const text = `Rarity: ${rarity}\n${name}\nImplicits: 0\nSapphire Ring`;
  render(text); harness.effect!();
  await vi.waitFor(() => expect(harness.states[0]).not.toBeNull());
  expect(render(text)).toContain('Use advanced text editing to modify it.');
  expect(harness.normalize).not.toHaveBeenCalled();
  expect(harness.workbench).not.toHaveBeenCalled();
});

it('explains an unavailable base without calling normalization', async () => {
  const text = 'Rarity: RARE\nVictory Ring\nUnknown Ring\nImplicits: 0';
  render(text); harness.effect!();
  await vi.waitFor(() => expect(harness.states[0]).not.toBeNull());
  expect(render(text)).toContain('Use advanced text editing to modify it.');
  expect(harness.normalize).not.toHaveBeenCalled();
  expect(harness.workbench).not.toHaveBeenCalled();
});

it('discards a late normalization result after closing the crafter', async () => {
  let resolve!: (text: string) => void;
  const pending = new Promise<string>(done => { resolve = done; });
  harness.normalize.mockReturnValue(pending);
  render(); const cleanup = harness.effect!();
  await vi.waitFor(() => expect(harness.normalize).toHaveBeenCalled());
  cleanup(); resolve(original);
  await pending;
  await new Promise(done => setTimeout(done, 0));
  expect(harness.states[0]).toBeNull();
  expect(harness.workbench).not.toHaveBeenCalled();
});

it('reports a failed item load', async () => {
  harness.normalize.mockRejectedValue(new Error('invalid item'));
  render(); harness.effect!();
  await vi.waitFor(() => expect(harness.states[1]).toBe(original));
  expect(render()).toContain('role="alert"');
});
