import type { ComponentProps, ReactElement } from 'react';
import { beforeEach, expect, test, vi } from 'vitest';
import type { PassiveNode } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import type { Lang } from '../../lib/i18n';
import { TreePanel } from './TreePanel';
import { passiveNodeMatches, usePassiveNodeNames, type PassiveNodeNames } from './passiveNodeNames';

const backend = vi.hoisted(() => ({ loadPassiveTree: vi.fn(), translateLines: vi.fn() }));
const hooks = await vi.hoisted(async () => {
  const { createHookHarness } = await import('../../testing/hookHarness');
  return createHookHarness({ refs: true, memo: 'deps', effects: { comparison: 'objectIs', cleanup: 'previous' } });
});
vi.mock('react', async original => hooks.mockReact(await original<typeof import('react')>()));
vi.mock('../../api/backend', () => ({ getBackend: async () => backend }));
vi.mock('../../hooks/useLocalizedLines', async original => ({
  ...await original<typeof import('../../hooks/useLocalizedLines')>(),
  useLocalizedNames: (names: { name: string }[]) => names.map(entry => entry.name),
}));
vi.mock('./TreeOptimizer', () => ({ TreeOptimizer: () => null, useTreeOptimizer: () => ({}) }));
vi.mock('./TreeConversionControls', () => ({ TreeConversionControls: () => null, useTreeConversion: () => ({}) }));

const nodes: PassiveNode[] = [{ skill: 1, id: 'critical', name: 'Critical Power', kind: 'normal',
  stats: ['10% increased Critical Hit Chance'] }];
const session = { stateVersion: 0, allocatedNodes: [], activeWeaponSet: 1,
  calc: { tree_effects: { nodes: {} } } } as unknown as BuildSession;
const copyNodes = () => nodes.map(node => ({ ...node, stats: [...node.stats!] }));
const render = <T,>(make: () => T): T => { hooks.beginRender(); return make(); };
const flush = async () => { hooks.runEffects(); await new Promise(resolve => setTimeout(resolve, 0)); };
interface CanvasProps { nodes: PassiveNode[]; nodeNames: PassiveNodeNames; onSearch: (query: string) => void }
const renderPanel = (current: BuildSession, lang: Lang) => render(() => {
  const element = TreePanel({ session: current, lang, active: true })!;
  // Exercise the editable parent without mounting the SVG canvas or planner UI.
  const Editable = element.type as (props: ComponentProps<typeof TreePanel>) => ReactElement<CanvasProps>;
  return Editable(element.props).props;
});

beforeEach(() => {
  hooks.reset();
  backend.loadPassiveTree.mockReset().mockResolvedValue(nodes);
  backend.translateLines.mockReset().mockImplementation(async (lines: string[]) => lines.map(line =>
    line.includes('Critical') ? '暴击几率提高 10%' : line.includes('Spirit') ? '精魂提高 30%' : line));
});

test.each(['en-US', 'zh-CN', 'zh-TW'] as const)('equivalent node changes across an allocation keep display nodes and stat translations in %s', async lang => {
  renderPanel(session, lang);
  await flush();
  let canvas = renderPanel(session, lang);
  canvas.onSearch('暴击');
  renderPanel(session, lang);
  await flush();
  canvas = renderPanel(session, lang);
  const displayed = canvas.nodes;
  const calls = lang === 'en-US' ? 0 : 1;
  expect(backend.translateLines).toHaveBeenCalledTimes(calls);
  const allocating = { ...session, allocatedNodes: [1], stateVersion: 1, busy: true, calc: null };
  canvas = renderPanel(allocating, lang);
  expect(canvas.nodes).toBe(displayed);
  await flush();
  canvas = renderPanel({ ...allocating, busy: false, calc: { tree_effects: { nodes: {} } } as BuildSession['calc'] }, lang);
  expect(canvas.nodes).toBe(displayed);
  await flush();
  expect(backend.translateLines).toHaveBeenCalledTimes(calls);
  expect(passiveNodeMatches(nodes[0], '暴击', canvas.nodeNames)).toBe(lang !== 'en-US');
});

test.each(['zh-CN', 'zh-TW'] as const)('stat cache survives recreated arrays and hidden or cleared searches in %s', async lang => {
  const read = (active: boolean, search: string, locale = lang) =>
    render(() => usePassiveNodeNames(copyNodes(), locale, active, search));
  read(false, '暴击'); await flush();
  read(true, ' '); await flush();
  render(() => usePassiveNodeNames(copyNodes(), 'en-US', true, 'Critical')); await flush();
  expect(backend.translateLines).not.toHaveBeenCalled();
  read(true, '暴击'); await flush();
  let names = read(true, '暴击');
  expect(passiveNodeMatches(nodes[0], '暴击', names)).toBe(true);
  await flush();
  names = read(false, ''); await flush();
  expect(passiveNodeMatches(nodes[0], '暴击', names)).toBe(true);
  read(true, 'Critical'); await flush();
  expect(backend.translateLines).toHaveBeenCalledTimes(1);
  const other = lang === 'zh-CN' ? 'zh-TW' : 'zh-CN';
  read(true, '精魂', other); await flush();
  expect(backend.translateLines).toHaveBeenCalledTimes(2);
  names = read(true, '暴击'); await flush();
  expect(passiveNodeMatches(nodes[0], '暴击', names)).toBe(true);
  expect(backend.translateLines).toHaveBeenCalledTimes(2);
});

test('changed conquered stats refresh the cache and removing the transformation restores the original search', async () => {
  renderPanel(session, 'zh-CN'); await flush();
  renderPanel(session, 'zh-CN').onSearch('精魂');
  renderPanel(session, 'zh-CN'); await flush();
  const original = renderPanel(session, 'zh-CN');
  expect(passiveNodeMatches(original.nodes[0], '暴击', original.nodeNames)).toBe(true);
  const conquered = () => ({ ...session, calc: { ...session.calc!, tree_effects: { ...session.calc!.tree_effects!, nodes: {
    1: { name: 'Conquered Power', stats: ['30% increased Spirit'], replace: true },
  } } } });
  let canvas = renderPanel(conquered(), 'zh-CN');
  expect(canvas.nodes).not.toBe(original.nodes);
  expect(passiveNodeMatches(canvas.nodes[0], '暴击', canvas.nodeNames)).toBe(false);
  await flush();
  const transformed = renderPanel(conquered(), 'zh-CN');
  expect(passiveNodeMatches(transformed.nodes[0], '精魂', transformed.nodeNames)).toBe(true);
  expect(renderPanel(conquered(), 'zh-CN').nodes).toBe(transformed.nodes);
  await flush();
  expect(backend.translateLines).toHaveBeenCalledTimes(2);
  canvas = renderPanel(session, 'zh-CN');
  expect(passiveNodeMatches(canvas.nodes[0], '精魂', canvas.nodeNames)).toBe(false);
  await flush();
  canvas = renderPanel(session, 'zh-CN');
  expect(passiveNodeMatches(canvas.nodes[0], '暴击', canvas.nodeNames)).toBe(true);
  expect(backend.translateLines).toHaveBeenCalledTimes(3);
});
