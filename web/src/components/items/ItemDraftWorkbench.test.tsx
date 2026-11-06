import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, it, vi } from 'vitest';
import type { BuildSession } from '../../hooks/useBuildSession';
import type { TradeBase } from '../../lib/tradeOptimizer';
import { ItemDraftWorkbench } from './ItemDraftWorkbench';
import { ReplacementAffixEditor } from '../trade/ReplacementAffixEditor';

const ui = vi.hoisted(() => ({ editor: undefined as ComponentProps<typeof ReplacementAffixEditor> | undefined, compare: vi.fn() }));
const hooks = await vi.hoisted(async () => {
  const { createHookHarness } = await import('../../testing/hookHarness');
  return createHookHarness({ lazyState: false, effects: { comparison: 'strict' } });
});
vi.mock('react', async original => hooks.mockReact(await original<typeof import('react')>()));
vi.mock('../../api/backend', () => ({ getBackend: async () => ({ normalizeItemDraft: async (text: string) => text,
  classifyItemLines: async () => [], loadPassiveTree: async () => [] }) }));
vi.mock('../trade/ReplacementAffixEditor', () => ({ ReplacementAffixEditor: (props: ComponentProps<typeof ReplacementAffixEditor>) => {
  ui.editor = props;
  return <details open={props.initiallyOpen}><summary>Edit modifiers</summary><button>{props.compareLabel}</button></details>;
} }));
vi.mock('../trade/ItemReplacementCheck', () => ({ ItemReplacementCheck: (props: unknown) => { ui.compare(props); return <div>Swap preview</div>; } }));
beforeEach(() => { hooks.reset(); ui.compare.mockClear(); ui.editor = undefined; });
const base: TradeBase = { name: 'Sapphire Ring', category: 'accessory.ring', tags: [], implicits: [], level: 1 };
const session = { items: [], busy: false } as unknown as BuildSession;
function render() {
  hooks.beginRender();
  const html = renderToStaticMarkup(<ItemDraftWorkbench initialText="Rarity: RARE\nDraft\nSapphire Ring" base={base}
    catalog={{ bases: [base], mods: [] }} session={session} lang="en-US" />);
  hooks.runEffects();
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
  expect(ui.compare).toHaveBeenLastCalledWith(expect.objectContaining({ objective: expect.objectContaining({ stat: 'CombinedDPS' }) }));
});

it('resolves a crafted-item preview to the main minion damage goal', async () => {
  session.calc = { stats: [], main_minion_index: 2, minions: [{ index: 2 }] } as unknown as BuildSession['calc'];
  render(); await vi.waitFor(() => expect(render()).toContain('Swap preview'));
  expect(ui.compare).toHaveBeenLastCalledWith(expect.objectContaining({ objective: expect.objectContaining({ stat: 'MainMinionDPS' }) }));
  session.calc = null;
});
