import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, it, vi } from 'vitest';
import type { BuildSession } from '../../hooks/useBuildSession';
import type { TradeBase } from '../../lib/tradeOptimizer';
import { ItemDraftWorkbench } from './ItemDraftWorkbench';
import { ReplacementAffixEditor } from '../trade/ReplacementAffixEditor';

const ui = vi.hoisted(() => ({ values: [] as unknown[], cursor: 0, effects: [] as (() => void)[],
  editor: undefined as ComponentProps<typeof ReplacementAffixEditor> | undefined, compare: vi.fn() }));
vi.mock('react', async original => ({ ...await original<typeof import('react')>(),
  useState: (initial: unknown) => {
    const index = ui.cursor++;
    if (!(index in ui.values)) ui.values[index] = initial;
    return [ui.values[index], (value: unknown) => { ui.values[index] = typeof value === 'function' ? value(ui.values[index]) : value; }];
  },
  useEffect: (callback: () => void, deps: unknown[]) => {
    const index = ui.cursor++;
    const previous = ui.values[index] as unknown[] | undefined;
    if (!previous || deps.some((value, i) => value !== previous[i])) { ui.values[index] = deps; ui.effects.push(callback); }
  },
}));
vi.mock('../../api/backend', () => ({ getBackend: async () => ({ normalizeItemDraft: async (text: string) => text,
  classifyItemLines: async () => [], loadPassiveTree: async () => [] }) }));
vi.mock('../trade/ReplacementAffixEditor', () => ({ ReplacementAffixEditor: (props: ComponentProps<typeof ReplacementAffixEditor>) => {
  ui.editor = props;
  return <details open={props.initiallyOpen}><summary>Edit modifiers</summary><button>{props.compareLabel}</button></details>;
} }));
vi.mock('../trade/ItemReplacementCheck', () => ({ ItemReplacementCheck: (props: unknown) => { ui.compare(props); return <div>Swap preview</div>; } }));
beforeEach(() => { ui.values = []; ui.effects = []; ui.compare.mockClear(); ui.editor = undefined; });
const base: TradeBase = { name: 'Sapphire Ring', category: 'accessory.ring', tags: [], implicits: [], level: 1 };
const session = { items: [], busy: false } as unknown as BuildSession;
function render() {
  ui.cursor = 0;
  const html = renderToStaticMarkup(<ItemDraftWorkbench initialText="Rarity: RARE\nDraft\nSapphire Ring" base={base}
    catalog={{ bases: [base], mods: [] }} session={session} lang="en-US" />);
  ui.effects.splice(0).forEach(effect => effect());
  return html;
}

it('opens modifier editing for a new draft and compares explicitly with current equipment', async () => {
  render(); await vi.waitFor(() => expect(render()).toContain('Swap preview'));
  expect(render()).toContain('<details open="">');
  expect(ui.editor?.compareLabel).toBe('Compare with current equipment');
  ui.editor!.onDirtyChange(true);
  expect(render()).not.toContain('Swap preview');
  ui.editor!.onCompare('Rarity: RARE\nEdited\nSapphire Ring\n+50 to maximum Life');
  render();
  await vi.waitFor(() => expect(render()).toContain('Swap preview'));
  expect(ui.compare).toHaveBeenLastCalledWith(expect.objectContaining({ initialItemText: expect.stringContaining('+50 to maximum Life') }));
});
