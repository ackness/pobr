import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { BuildSession } from '../../hooks/useBuildSession';
import { ItemsPanel } from './ItemsPanel';

// Replay native handlers with state retained across SSR renders, without a browser dependency.
const harness = vi.hoisted(() => ({ states: [] as unknown[], cursor: 0, buttons: [] as ComponentProps<'button'>[],
  crafter: vi.fn(), reforge: vi.fn(), craftUnavailable: false, craftLoading: false }));
vi.mock('react', async original => ({ ...await original<typeof import('react')>(),
  useState: (initial: unknown) => {
    const index = harness.cursor++;
    if (!(index in harness.states)) harness.states[index] = typeof initial === 'function' ? initial() : initial;
    return [harness.states[index], (next: unknown) => { harness.states[index] = typeof next === 'function' ? next(harness.states[index]) : next; }];
  },
}));
vi.mock('../shared/controls', () => ({
  AppButton: (props: ComponentProps<'button'>) => { harness.buttons.push(props); return <button {...props} />; },
  AppInput: (props: ComponentProps<'input'>) => <input {...props} />,
  AppTextarea: (props: ComponentProps<'textarea'>) => <textarea {...props} />,
}));
vi.mock('../../hooks/useLocalizedLines', async original => ({ ...await original<typeof import('../../hooks/useLocalizedLines')>(),
  useItemDisplayNames: (texts: (string | undefined)[]) => texts.map(text => text?.split('\n')[1] ?? ''),
  useLocalizedLines: (lines: string[]) => lines,
}));
vi.mock('../../api/backend', () => ({ getBackend: async () => ({ reforgeRunes: harness.reforge }) }));
vi.mock('./ItemCrafter', () => ({ ItemCrafter: () => null,
  useEquippedItemDraft: (text: string | undefined) => ({ error: false,
    draft: text && !harness.craftLoading ? { source: text, text, base: harness.craftUnavailable ? undefined : { name: 'Sapphire Ring' }, catalog: {} } : null }),
  EquippedItemCrafter: (props: unknown) => { harness.crafter(props); return <div>Crafting selected item</div>; },
}));
vi.mock('./UniqueCatalog', () => ({ UniqueCatalog: () => null }));
vi.mock('./ItemOptimizer', () => ({ ItemOptimizer: () => null }));
vi.mock('./ItemDisplay', () => ({ ItemDisplay: () => null }));
vi.mock('../shared/CopyButton', () => ({ CopyButton: () => <button>Copy</button> }));
vi.mock('../shared/NoteEditor', () => ({ NoteEditor: () => null }));
vi.mock('../shared/WeaponSetControl', () => ({ WeaponSetControl: () => null }));
vi.mock('../shared/AppSelect', () => ({ AppSelect: () => null }));
vi.mock('../shared/PreviewContext', () => ({ usePreview: () => null }));

const text = 'Rarity: RARE\nVictory Ring\nSapphire Ring\n+50 to maximum Life';
function session() {
  return { items: [{ slot: 'ring1', text }], flasks: [], limbs: [], build: null, editorDrafts: {}, annotations: {},
    activeWeaponSet: 1, stateVersion: 1, character: { level: 100 }, busy: false,
    library: { items: [{ id: 'ring', kind: 'item', slot: 'ring1', name: 'Victory Ring', text }] },
    currentRequest: () => ({}), setItems: vi.fn(), setEditorDraft: vi.fn(), removeLibraryItem: vi.fn(),
  } as unknown as BuildSession;
}
function render(current: BuildSession) {
  harness.cursor = 0;
  harness.buttons.length = 0;
  return renderToStaticMarkup(<ItemsPanel session={current} lang="en-US" />);
}
function button(label: string) {
  const found = harness.buttons.find(props => props['aria-label'] === label || props.title === label || props.children === label);
  expect(found, label).toBeDefined();
  return found!;
}
beforeEach(() => {
  harness.states = [];
  harness.craftUnavailable = false;
  harness.craftLoading = false;
  harness.crafter.mockClear();
  vi.stubGlobal('requestAnimationFrame', (callback: () => void) => callback());
});
afterEach(() => vi.unstubAllGlobals());

it('explains disabled library actions and enables matching items after choosing a slot', () => {
  const current = session();
  expect(render(current)).toContain('id="library-slot-hint" class="items-hint">Select an equipment slot first');
  expect(button('Compare').disabled).toBe(true);
  expect(button('Equip').disabled).toBe(true);
  expect(button('Equip')['aria-describedby']).toBe('library-slot-hint');
  button('Ring 1').onClick!({} as never);
  expect(render(current)).not.toContain('id="library-slot-hint"');
  expect(button('Equip').disabled).toBe(false);
  button('Equip').onClick!({} as never);
  expect(current.setItems).toHaveBeenCalledWith([{ slot: 'ring1', text }]);
  button('Helmet').onClick!({} as never);
  render(current);
  expect(harness.buttons.some(props => props.children === 'Equip')).toBe(false);
});

it('opens the crafter with the equipped text, leaves equipment untouched and closes on a second click', () => {
  const current = session();
  render(current);
  button('Ring 1').onClick!({} as never);
  render(current);
  expect(button('Edit text (advanced)')).toBeDefined();
  button('Edit in item crafter').onClick!({} as never);
  expect(render(current)).toContain('Crafting selected item');
  expect(harness.crafter).toHaveBeenLastCalledWith(expect.objectContaining({ draft: expect.objectContaining({ text }), session: current }));
  expect(current.setItems).not.toHaveBeenCalled();
  button('Edit in item crafter').onClick!({} as never);
  expect(render(current)).not.toContain('Crafting selected item');
});

it('labels socket controls and retains the grafted-limb explanation', () => {
  const current = session();
  render(current);
  button('Ring 1').onClick!({} as never);
  // Metadata for this exact selected item, as supplied by the backend.
  harness.states[5] = { slot: 'ring1', text, catalog: [], info: { editable: true, sockets: 1, max_sockets: 1, runes: [], options: [] } };
  const html = render(current);
  expect(html).toContain('− Socket');
  expect(html).toContain('+ Socket');
  expect(button('Add socket').disabled).toBe(true);
  expect(html).toContain('Configure grafted limbs and their effects here.');
});

it('disables the crafter entry until preparation succeeds and explains an unknown base', () => {
  const current = session();
  render(current);
  button('Ring 1').onClick!({} as never);
  harness.craftLoading = true;
  render(current);
  expect(button('Edit in item crafter').disabled).toBe(true);
  harness.craftLoading = false;
  harness.craftUnavailable = true;
  render(current);
  expect(button('Edit in item crafter').disabled).toBe(true);
  expect(button('Edit in item crafter').title).toContain('Use advanced text editing to modify it.');
  expect(harness.crafter).not.toHaveBeenCalled();
  harness.craftUnavailable = false;
  render(current);
  expect(button('Edit in item crafter').disabled).toBe(false);
});
