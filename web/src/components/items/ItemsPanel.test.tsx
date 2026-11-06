import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { BuildSession } from '../../hooks/useBuildSession';
import { ItemsPanel } from './ItemsPanel';

// Replay native handlers with state retained across SSR renders, without a browser dependency.
const harness = vi.hoisted(() => ({ buttons: [] as ComponentProps<'button'>[],
  toolKeys: [] as { name: string; key: string | null }[], inputs: [] as ComponentProps<'input'>[], compare: vi.fn(), newCrafter: vi.fn(), crafter: vi.fn(), reforge: vi.fn(), craftUnavailable: false, craftLoading: false }));
// Observe React's reconciliation identity at the real parent boundary; child
// state tests alone cannot catch a parent resetting the entire tool via its key.
vi.mock('react/jsx-runtime', async original => {
  const runtime = await original<typeof import('react/jsx-runtime')>();
  const observe = (create: typeof runtime.jsx): typeof runtime.jsx => (...args) => {
    const element = create(...args);
    if (typeof args[0] === 'function' && ['ItemCrafter', 'UniqueCatalog'].includes(args[0].name)) {
      harness.toolKeys.push({ name: args[0].name, key: element.key });
    }
    return element;
  };
  return { ...runtime, jsx: observe(runtime.jsx), jsxs: observe(runtime.jsxs) };
});
vi.mock('react/jsx-dev-runtime', async original => {
  const runtime = await original<typeof import('react/jsx-dev-runtime')>();
  return { ...runtime, jsxDEV: (...args: Parameters<typeof runtime.jsxDEV>) => {
    const element = runtime.jsxDEV(...args);
    if (typeof args[0] === 'function' && ['ItemCrafter', 'UniqueCatalog'].includes(args[0].name)) {
      harness.toolKeys.push({ name: args[0].name, key: element.key });
    }
    return element;
  } };
});
const hooks = await vi.hoisted(async () => {
  const { createHookHarness } = await import('../../testing/hookHarness');
  return createHookHarness();
});
vi.mock('react', async original => hooks.mockReact(await original<typeof import('react')>()));
vi.mock('../shared/controls', () => ({
  AppButton: (props: ComponentProps<'button'>) => { harness.buttons.push(props); return <button {...props} />; },
  AppInput: (props: ComponentProps<'input'>) => { harness.inputs.push(props); return <input {...props} />; },
  AppTextarea: (props: ComponentProps<'textarea'>) => <textarea {...props} />,
}));
vi.mock('../../hooks/useLocalizedLines', async original => ({ ...await original<typeof import('../../hooks/useLocalizedLines')>(),
  useItemDisplayNames: (texts: (string | undefined)[]) => texts.map(text => text?.split('\n')[1] ?? ''),
  useLocalizedLines: (lines: string[]) => lines,
  useLocalizedNames: (names: { name: string }[]) => names.map(entry => entry.name),
}));
vi.mock('../../api/backend', () => ({ getBackend: async () => ({ reforgeRunes: harness.reforge, calculateBuild: harness.compare }) }));
vi.mock('./ItemCrafter', () => ({ ItemCrafter: (props: unknown) => { harness.newCrafter(props); return null; },
  useEquippedItemDraft: (text: string | undefined) => ({ error: false,
    draft: text && !harness.craftLoading ? { source: text, text, base: harness.craftUnavailable ? undefined : { name: 'Sapphire Ring' }, catalog: {} } : null }),
  EquippedItemCrafter: (props: unknown) => { harness.crafter(props); return <div>Crafting selected item</div>; },
}));
vi.mock('./UniqueCatalog', () => ({ UniqueCatalog: () => null }));
vi.mock('./ItemOptimizer', () => ({ ItemOptimizer: () => null }));
vi.mock('./ItemContribution', () => ({ ItemContribution: () => null }));
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
function render(current: BuildSession, onUpgrade?: (slot: string) => void) {
  hooks.beginRender();
  harness.buttons.length = 0;
  harness.inputs.length = 0;
  harness.toolKeys.length = 0;
  return renderToStaticMarkup(<ItemsPanel session={current} lang="en-US" onUpgrade={onUpgrade} />);
}
function button(label: string) {
  const found = harness.buttons.find(props => props['aria-label'] === label || props.title === label || props.children === label);
  expect(found, label).toBeDefined();
  return found!;
}
beforeEach(() => {
  hooks.reset();
  harness.craftUnavailable = false;
  harness.craftLoading = false;
  harness.crafter.mockClear();
  harness.compare.mockReset();
  harness.newCrafter.mockClear();
  vi.stubGlobal('requestAnimationFrame', (callback: () => void) => callback());
});
afterEach(() => vi.unstubAllGlobals());

it('offers trade search in item details and hands off the inspected slot without editing equipment', () => {
  const current = session(), onUpgrade = vi.fn();
  expect(render(current, onUpgrade)).not.toContain('Find a better one on the trade site');
  button('Ring 1').onClick!({} as never);
  const html = render(current, onUpgrade);
  expect(html.indexOf('Find a better one on the trade site')).toBeGreaterThan(html.indexOf('class="item-detail-header"'));
  button('Find a better one on the trade site').onClick!({} as never);
  expect(onUpgrade).toHaveBeenCalledWith('ring1');
  expect(current.setItems).not.toHaveBeenCalled();
});

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
  hooks.slots[5] = { slot: 'ring1', text, catalog: [], info: { editable: true, sockets: 1, max_sockets: 1, runes: [], options: [] } };
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


it('shows saved source builds and legacy unknown sources, and filters by build identity', () => {
  const current = session();
  const old = current.library.items[0];
  current.workspace = { version: 1, activeBuild: 'a', builds: [{ id: 'a', name: 'Current name', stages: [], activeStage: '' }] };
  current.library.items = [old,
    { ...old, id: 'a-item', name: 'Build A Ring', text: old.text.replace('Victory Ring', 'Build A Ring'), sourceBuild: { id: 'a', name: 'Old name' } },
    { ...old, id: 'b-item', name: 'Build B Ring', text: old.text.replace('Victory Ring', 'Build B Ring'), sourceBuild: { id: 'b', name: 'Build B' } }];
  const html = render(current);
  expect(html).toContain('Source unknown');
  expect(html).toContain('Source build: Current name');
  expect(html).toContain('Source build: Build B');
  const filter = harness.inputs.find(input => input['aria-label'] === 'This build only')!;
  filter.onChange!({ target: { checked: true } } as never);
  const filtered = render(current);
  expect(filtered).toContain('Build A Ring');
  expect(filtered).not.toContain('Build B Ring');
  expect(filtered).not.toContain('Source unknown');
  current.workspace.activeBuild = 'b';
  const next = render(current);
  expect(next).toContain('Build B Ring');
  expect(next).not.toContain('Build A Ring');
});

it('passes the selected main-hand slot into the new-item crafter', () => {
  const current = session();
  render(current); button('Main Hand').onClick!({} as never);
  render(current); button('Craft item').onClick!({} as never);
  render(current);
  expect(harness.newCrafter).toHaveBeenLastCalledWith(expect.objectContaining({ selectedSlot: 'weapon1' }));
});

it('renders library comparisons through the shared before/after card and invalidates them on build changes', async () => {
  const current = session();
  current.calc = { stats: [{ id: 'CombinedDPS', value: 100 }, { id: 'TotalEHP', value: 200 }, { id: 'Life', value: 50 }] } as BuildSession['calc'];
  harness.compare.mockResolvedValue({ stats: [{ id: 'CombinedDPS', value: 150 }, { id: 'TotalEHP', value: 180 }, { id: 'Life', value: 100 }] });
  render(current); button('Ring 1').onClick!({} as never);
  render(current); button('Compare').onClick!({} as never);
  await vi.waitFor(() => expect(harness.compare).toHaveBeenCalledOnce());
  await Promise.resolve();
  const html = render(current);
  expect(html).toContain('data-stat="CombinedDPS"');
  expect(html).toContain('+50.0%');
  expect(html).toContain('100</span> → <b class="swap-after">150');
  current.currentRequest = () => ({ items: [] });
  expect(render(current)).not.toContain('class="swap-gain-card"');
});

it('passes the main minion headline and per-minion note to a saved-item comparison', async () => {
  const current = session();
  current.calc = { stats: [{ id: 'CombinedDPS', value: 0 }, { id: 'MainMinionDPS', value: 100 }],
    main_minion_index: 2, minions: [{ index: 2, name: 'Wolf' }] } as BuildSession['calc'];
  harness.compare.mockResolvedValue({ stats: [{ id: 'CombinedDPS', value: 0 }, { id: 'MainMinionDPS', value: 150 }] });
  render(current); button('Ring 1').onClick!({} as never);
  render(current); button('Compare').onClick!({} as never);
  await vi.waitFor(() => expect(harness.compare).toHaveBeenCalledOnce());
  await Promise.resolve();
  const html = render(current);
  expect(html).toContain('data-stat="MainMinionDPS"');
  expect(html).toContain('+50.0%');
  expect(html).toContain('Wolf · per minion');
});


it.each(['Craft item', 'Unique catalogue'])('keeps %s mounted with the same identity while inspecting other slots', label => {
  const current = session();
  render(current); button('Ring 1').onClick!({} as never);
  render(current); button(label).onClick!({} as never);
  render(current);
  const identity = [...harness.toolKeys];
  expect(identity).toHaveLength(1);
  button('Main Hand').onClick!({} as never);
  render(current);
  expect(harness.toolKeys).toEqual(identity);
});
