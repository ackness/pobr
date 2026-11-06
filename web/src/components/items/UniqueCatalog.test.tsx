import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, it, vi } from 'vitest';
import type { BuildSession } from '../../hooks/useBuildSession';
import type { UniqueSelection } from '../../api/uniques';
import { UniqueCatalog } from './UniqueCatalog';

const ui = vi.hoisted(() => ({ values: [] as unknown[], cursor: 0, effects: [] as (() => void)[],
  buttons: [] as ComponentProps<'button'>[], ranges: [] as { onValueChange: (value: number) => void }[],
  selects: [] as { ariaLabel: string; onChange: (value: string) => void; value: string }[],
  draft: vi.fn(), workbench: vi.fn(), cleanups: [] as (() => void)[] }));
vi.mock('react', async original => ({ ...await original<typeof import('react')>(),
  useState: (initial: unknown) => {
    const index = ui.cursor++;
    if (!(index in ui.values)) ui.values[index] = typeof initial === 'function' ? initial() : initial;
    return [ui.values[index], (value: unknown) => { ui.values[index] = typeof value === 'function' ? value(ui.values[index]) : value; }];
  },
  useRef: (initial: unknown) => {
    const index = ui.cursor++;
    return ui.values[index] ??= { current: initial };
  },
  useEffect: (callback: () => void | (() => void), deps: unknown[]) => {
    const index = ui.cursor++;
    const previous = ui.values[index] as unknown[] | undefined;
    if (!previous || deps.some((value, i) => value !== previous[i])) {
      ui.values[index] = deps;
      ui.effects.push(() => { ui.cleanups[index]?.(); ui.cleanups[index] = callback() || (() => {}); });
    }
  },
}));
vi.mock('../../hooks/useLocalizedLines', () => ({ useLocalizedLines: (lines: string[]) => lines,
  useLocalizedNames: (names: { name: string }[]) => names.map(name => name.name) }));
vi.mock('../shared/controls', () => ({
  AppButton: (props: ComponentProps<'button'>) => { ui.buttons.push(props); return <button {...props} />; },
  AppInput: (props: ComponentProps<'input'>) => <input {...props} />,
}));
vi.mock('../shared/AppSelect', () => ({ AppSelect: (props: typeof ui.selects[number]) => { ui.selects.push(props); return <span>{props.value}</span>; } }));
vi.mock('../shared/AppRange', () => ({ AppRange: (props: typeof ui.ranges[number]) => { ui.ranges.push(props); return <input type="range" />; } }));
vi.mock('./ItemDraftWorkbench', () => ({ ItemDraftWorkbench: (props: unknown) => { ui.workbench(props); return <div>Preview</div>; } }));
const ring = { id: 1, name: 'Example Ring', item_type: 'ring', bases: [{ name: 'Iron Ring', item_class: 'Ring' }],
  raw: 'Example Ring\nIron Ring\nImplicits: 0\n+(10-20) to maximum Life' };
const bow = { id: 2, name: 'Example Bow', item_type: 'bow', bases: [{ name: 'Simple Bow', item_class: 'Bow' }],
  raw: 'Example Bow\nSimple Bow\nImplicits: 0\n+(10-20) to maximum Life' };
const catalog = { bases: [{ name: 'Iron Ring', category: 'accessory.ring', implicits: [], tags: [], level: 1 },
  { name: 'Simple Bow', category: 'weapon.bow', implicits: [], tags: [], level: 1 }], mods: [] };
vi.mock('../../api/backend', () => ({ getBackend: async () => ({ uniqueCatalog: async () => [ring, bow],
  itemCraftingCatalog: async () => ({ bases: [], mods: [] }), uniqueItemDraft: ui.draft }) }));
vi.mock('../../lib/tradeOptimizer', async original => ({ ...await original<typeof import('../../lib/tradeOptimizer')>(), loadTradeCatalog: async () => catalog }));
const session = { items: [], treeVersion: null, busy: false } as unknown as BuildSession;
function render(selectedSlot = 'weapon1') {
  ui.cursor = 0; ui.buttons = []; ui.selects = []; ui.ranges = [];
  const html = renderToStaticMarkup(<UniqueCatalog selectedSlot={selectedSlot} session={session} lang="en-US" />);
  ui.effects.splice(0).forEach(effect => effect());
  return html;
}
beforeEach(() => {
  ui.values = []; ui.effects = []; ui.cleanups = []; ui.workbench.mockClear();
  ui.draft.mockReset().mockImplementation(async (selection: UniqueSelection) => ({ base: 'Simple Bow', required_level: 1,
    text: `Rarity: UNIQUE\nExample Bow\nSimple Bow\n+${10 + (selection.rolls[3] ?? .5) * 10} to maximum Life` }));
});

it('defaults to the selected slot and exposes a list with base names and equipment slots', async () => {
  render();
  await vi.waitFor(() => expect(render()).toContain('Example Bow'));
  const html = render();
  expect(html).not.toContain('Example Ring');
  expect(html).toContain('Simple Bow');
  expect(html).toContain('Main Hand');
  expect(html).toContain('class="unique-results" role="group"');
  expect(ui.selects.find(select => select.ariaLabel === 'Equipment slot')?.value).toBe('weapon1');
  ui.selects.find(select => select.ariaLabel === 'Equipment slot')!.onChange('');
  expect(render()).toContain('Example Ring');
});

it('updates the actual modifier value from the backend when a unique roll changes, without applying equipment', async () => {
  render(); await vi.waitFor(() => expect(render()).toContain('Example Bow'));
  ui.buttons.find(button => button.className === 'unique-result')!.onClick!({} as never);
  render();
  await vi.waitFor(() => expect(render()).toContain('+15 to maximum Life'));
  expect(render()).toContain('50%');
  ui.ranges[0].onValueChange(100);
  const loading = render();
  expect(loading).not.toContain('+15 to maximum Life');
  await vi.waitFor(() => expect(render()).toContain('+20 to maximum Life'));
  expect(render()).toContain('100%');
  expect(ui.draft).toHaveBeenLastCalledWith(expect.objectContaining({ rolls: { 3: 1 } }));
  expect(ui.workbench).not.toHaveBeenCalled();
});


it('follows the inspected slot until a selection is made, then preserves the selected unique and its rolls', async () => {
  render(); await vi.waitFor(() => expect(render()).toContain('Example Bow'));
  expect(render('ring1')).toContain('Example Ring');
  expect(ui.selects.find(select => select.ariaLabel === 'Equipment slot')?.value).toBe('ring1');
  render('weapon1');
  ui.buttons.find(button => button.className === 'unique-result')!.onClick!({} as never);
  render(); await vi.waitFor(() => expect(render()).toContain('+15 to maximum Life'));
  ui.ranges[0].onValueChange(100);
  render(); await vi.waitFor(() => expect(render()).toContain('+20 to maximum Life'));
  const html = render('ring1');
  expect(ui.selects.find(select => select.ariaLabel === 'Equipment slot')?.value).toBe('weapon1');
  expect(html).toContain('+20 to maximum Life');
  expect(html).toContain('100%');
  expect(html).not.toContain('Example Ring');
});

it('preserves an explicit catalogue slot filter during slot inspection', async () => {
  render(); await vi.waitFor(() => expect(render()).toContain('Example Bow'));
  ui.selects.find(select => select.ariaLabel === 'Equipment slot')!.onChange('');
  const html = render('ring1');
  expect(ui.selects.find(select => select.ariaLabel === 'Equipment slot')?.value).toBe('');
  expect(html).toContain('Example Bow');
  expect(html).toContain('Example Ring');
});
