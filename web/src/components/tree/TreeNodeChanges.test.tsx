import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import type { CalculateBuildResponse, TreeEditResponse } from '../../api/types';
import { TreeNodeChanges } from './TreeNodeChanges';

const ui = vi.hoisted(() => ({ value: null as unknown, open: false, effects: [] as (() => void | (() => void))[],
  deps: [] as unknown[], cleanup: undefined as (() => void) | undefined, calculate: vi.fn() }));
vi.mock('react', async original => ({ ...await original<typeof import('react')>(),
  useState: (initial: unknown) => typeof initial === 'boolean'
    ? [ui.open, (next: boolean) => { ui.open = next; }]
    : [ui.value, (next: unknown) => { ui.value = next; }],
  useMemo: (make: () => unknown) => make(),
  useEffect: (effect: () => void | (() => void), deps: unknown[]) => {
    if (deps.some((value, index) => value !== ui.deps[index])) {
      ui.cleanup?.(); ui.deps = deps; ui.effects.push(effect);
    }
  },
}));
vi.mock('../../api/backend', () => ({ getBackend: async () => ({ calculateBuild: ui.calculate }) }));
const result = (value: number) => ({ stats: [
  { id: 'CombinedDPS', value }, { id: 'TotalDPS', value }, { id: 'Life', value },
  { id: 'TotalEHP', value }, { id: 'Mana', value }, { id: 'Armour', value },
] }) as CalculateBuildResponse;
const preview: TreeEditResponse = { valid: true, reason: null, result: { allocation: { shared: [1], exclusive: [[], []] },
  removed: [2, 3], added: [], promoted: [], path: [], points_used: 0, points_max: 99,
  weapon_points_used: [0, 0], weapon_points_max: 24 } };
const props = { request: { allocated_nodes: [1, 2, 3] }, baseline: result(100), preview,
  busy: false, allocated: true, lang: 'en-US' as const };
const render = (next: ComponentProps<typeof TreeNodeChanges> = props) => renderToStaticMarkup(TreeNodeChanges(next));
const toggle = (open: boolean, next: ComponentProps<typeof TreeNodeChanges> = props) => {
  TreeNodeChanges(next).props.onToggle({ currentTarget: { open } } as never);
  return render(next);
};
const flush = async () => {
  ui.effects.splice(0).forEach(effect => { ui.cleanup?.(); ui.cleanup = effect() ?? undefined; });
  await vi.advanceTimersByTimeAsync(300);
};
beforeEach(() => { ui.value = null; ui.open = false; ui.deps = []; ui.effects = []; ui.cleanup = undefined; ui.calculate.mockReset(); vi.useFakeTimers(); });
afterEach(() => { ui.cleanup?.(); vi.useRealTimers(); });

test('selected-node details expose the same full refund changes through keyboard-operable summaries', async () => {
  ui.calculate.mockResolvedValue(result(80));
  expect(render()).toContain('calculating');
  await flush();
  expect(ui.calculate).not.toHaveBeenCalled();
  toggle(true);
  await flush();
  const html = render();
  expect(html).toContain('<details class="tree-node-changes"><summary class="disclosure">');
  expect(html).toContain('Combined DPS -20');
  expect(html).toContain('Also refunds 1 downstream nodes');
  expect(html).toContain('<summary class="disclosure">1 more changes</summary>');
  expect(ui.calculate).toHaveBeenCalledWith(expect.objectContaining({ allocated_nodes: [1] }));
});

test('selected-node changes discard stale results when the build or preview changes', async () => {
  let resolve!: (value: CalculateBuildResponse) => void;
  ui.calculate.mockImplementationOnce(() => new Promise<CalculateBuildResponse>(done => { resolve = done; }));
  toggle(true); await flush();
  ui.calculate.mockResolvedValue(result(120));
  const next = { ...props, request: { allocated_nodes: [1, 2, 3, 4] }, baseline: result(200) };
  expect(render(next)).toContain('calculating');
  await flush();
  resolve(result(50)); await Promise.resolve();
  const html = render(next);
  expect(html).toContain('Combined DPS -80');
  expect(html).not.toContain('Combined DPS -50');
});


test('attribute selection recalculates the inspector with the chosen attribute and invalidates older choices', async () => {
  const allocationPreview: TreeEditResponse = { ...preview, result: { ...preview.result!,
    allocation: { shared: [1, 2], exclusive: [[], []] }, removed: [], added: [2], path: [2] } };
  const selected = { ...props, allocated: false, preview: allocationPreview,
    request: { allocated_nodes: [1], attribute_choices: { '1': 'int' as const } } };
  const original = JSON.stringify(selected.request);
  let resolveStrength!: (value: CalculateBuildResponse) => void;
  ui.calculate.mockImplementationOnce(() => new Promise<CalculateBuildResponse>(resolve => { resolveStrength = resolve; }));
  toggle(true, { ...selected, attributeChoices: { '2': 'str' } }); await flush();
  expect(ui.calculate).toHaveBeenLastCalledWith(expect.objectContaining({
    allocated_nodes: [1, 2], attribute_choices: { '1': 'int', '2': 'str' },
  }));

  ui.calculate.mockResolvedValueOnce(result(120));
  const dexterity = { ...selected, attributeChoices: { '2': 'dex' as const } };
  expect(render(dexterity)).toContain('calculating');
  await flush();
  expect(ui.calculate).toHaveBeenLastCalledWith(expect.objectContaining({ attribute_choices: { '1': 'int', '2': 'dex' } }));
  expect(render(dexterity)).toContain('Combined DPS +20');
  resolveStrength(result(110)); await Promise.resolve();
  expect(render(dexterity)).toContain('Combined DPS +20');
  expect(render(dexterity)).not.toContain('Combined DPS +10');

  ui.calculate.mockResolvedValueOnce(result(130));
  const intelligence = { ...selected, attributeChoices: { '2': 'int' as const } };
  expect(render(intelligence)).toContain('calculating');
  expect(render(intelligence)).not.toContain('Combined DPS +20');
  await flush();
  expect(ui.calculate).toHaveBeenLastCalledWith(expect.objectContaining({ attribute_choices: { '1': 'int', '2': 'int' } }));
  expect(render(intelligence)).toContain('Combined DPS +30');
  expect(JSON.stringify(selected.request)).toBe(original);
});

test('completed changes survive pointer renders and reopening; closed pending requests are cancelled', async () => {
  ui.calculate.mockResolvedValue(result(80));
  toggle(true); await flush();
  expect(render()).toContain('Combined DPS -20');
  toggle(false); await flush();
  toggle(true); await flush();
  expect(ui.calculate).toHaveBeenCalledTimes(1);
  const next = { ...props, request: { ...props.request, attribute_choices: { '1': 'dex' as const } } };
  render(next);
  toggle(false, next); await flush();
  expect(ui.calculate).toHaveBeenCalledTimes(1);
});
