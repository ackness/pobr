import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import type { BuildSession } from '../../hooks/useBuildSession';
import type { PassiveJewelState, PassiveNode } from '../../api/types';
import type { ObjectiveState } from '../shared/OptimizerControls';
import { TreeOptimizer, useTreeOptimizer, type TreeOptimizerState } from './TreeOptimizer';

const ui = vi.hoisted(() => ({ plan: vi.fn(), jewel: vi.fn(), goal: { preset: 'dps', cStat: '', cMin: '', cMax: '' } as ObjectiveState }));
const hooks = await vi.hoisted(async () => {
  const { createHookHarness } = await import('../../testing/hookHarness');
  return createHookHarness({ refs: true, memo: 'deps', callback: 'deps',
    effects: { comparison: 'objectIs', cleanup: 'previous' }, layoutEffect: 'effect' });
});
vi.mock('react', async original => hooks.mockReact(await original<typeof import('react')>()));

vi.mock('../../app/jobs/useJobBeacon', () => ({ useJobBeacon: () => {} }));
vi.mock('../../hooks/useUpgradeGoal', () => ({ useUpgradeGoal: () => ({ goal: ui.goal, setGoal: vi.fn() }) }));
vi.mock('../../hooks/useLocalizedLines', () => ({ useLocalizedNames: (names: { name: string }[]) => names.map(entry => entry.name) }));
vi.mock('../../lib/passivePlanner', async original => ({ ...await original<typeof import('../../lib/passivePlanner')>(),
  planPassiveUpgrades: (...args: unknown[]) => ui.plan(...args),
}));
vi.mock('./JewelPassivePlanner', () => ({ JewelPassivePlanner: () => null,
  useJewelPassivePlanner: (props: unknown) => { ui.jewel(props); return { stop: vi.fn(), clearPreview: vi.fn() }; },
}));
vi.mock('../shared/AppSelect', () => ({ AppSelect: () => null }));
vi.mock('./PlannerResultsNotice', () => ({ PlannerResultsNotice: ({ children }: { children: unknown }) => children }));

const nodes = [{ skill: 1, id: '1', name: 'WITCH', connections: [2], kind: 'normal', x: 0, y: 0 },
  { skill: 2, id: '2', name: 'Minion power', connections: [1], kind: 'notable', x: 1, y: 1 }] as PassiveNode[];
const effects: PassiveJewelState = { class_starts: { witch: 1, wayfarer: 1 }, allocation_grants: [],
  nodes: {}, conquered: [], unresolved: [], rings: [], warnings: [] };
let session: BuildSession;
let planner: TreeOptimizerState;
const focusPlanner = { nonce: 1 }, onPreview = vi.fn(), nodeLabel = () => 'Minion power';
function Harness() {
  planner = useTreeOptimizer({ session, nodes, lang: 'en-US', active: true, focusPlanner, onPreview, nodeLabel });
  return <TreeOptimizer planner={planner} />;
}
function render() {
  for (let count = 0; count < 10; count++) {
    hooks.beginRender();
    const html = renderToStaticMarkup(<Harness />);
    hooks.runEffects();
    if (!hooks.dirty) return html;
  }
  throw new Error('Render did not settle');
}
beforeEach(() => {
  hooks.reset(); ui.plan.mockReset(); ui.jewel.mockClear();
  ui.goal = { preset: 'dps', cStat: '', cMin: '', cMax: '' };
  vi.useFakeTimers(); vi.stubGlobal('window', { setTimeout, clearTimeout });
  const request = { allocated_nodes: [1] };
  session = { currentRequest: () => request, allocatedNodes: [1], attributeChoices: {}, jewels: [],
    character: { class_name: 'Witch' }, activeWeaponSet: 1, stateVersion: 1, calc: { stats: [], tree_effects: effects },
  } as unknown as BuildSession;
  ui.plan.mockResolvedValue({ baseline: {}, plans: [], evaluated: 1, limited: false, unmodeled: 0, baselineUnsupported: [], excluded: [] });
});
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

test.each(['dps', 'balanced', 'dpsPerMana'])('passes the resolved %s goal into both passive planners for normal and minion builds', async preset => {
  ui.goal.preset = preset;
  render(); planner.run(); render();
  await vi.advanceTimersByTimeAsync(350); render();
  expect(ui.plan).toHaveBeenLastCalledWith(expect.objectContaining({ objective: expect.objectContaining({
    stat: preset === 'dpsPerMana' ? 'TotalDPS' : 'CombinedDPS',
  }) }));
  session.calc = { ...session.calc!, stats: [], main_minion_index: 2, minions: [{ index: 2, name: 'Wolf' }] } as unknown as BuildSession['calc'];
  render(); await vi.advanceTimersByTimeAsync(350); render();
  const objective = { stat: preset === 'dpsPerMana' ? 'TotalDPS' : 'MainMinionDPS',
    ...(preset === 'dpsPerMana' ? { per: 'ManaCost' } : {}) };
  expect(ui.plan).toHaveBeenLastCalledWith(expect.objectContaining({ objective: expect.objectContaining(objective) }));
  expect(ui.jewel).toHaveBeenLastCalledWith(expect.objectContaining({ objective: expect.objectContaining(objective) }));
});

test('a minion-only passive improvement shows its damage gain, baseline and per-minion note', async () => {
  session.calc = { stats: [{ id: 'ActiveMinionLimit', value: 7 }], tree_effects: effects, main_minion_index: 2,
    minions: [{ index: 2, name: 'Wolf' }] } as unknown as BuildSession['calc'];
  ui.plan.mockResolvedValue({ baseline: { TotalDPS: 0, CombinedDPS: 0, MainMinionDPS: 100 },
    plans: [{ allocate: [2], deallocate: [], target: 2, stats: { TotalDPS: 0, CombinedDPS: 0, MainMinionDPS: 150 } }],
    evaluated: 1, limited: false, unmodeled: 0, baselineUnsupported: [], excluded: [] });
  render(); planner.run(); render(); await vi.advanceTimersByTimeAsync(350);
  const html = render()!;
  expect(html).toContain('Minion hit DPS 100');
  expect(html).toMatch(/Minion hit DPS <b[^>]*>\+50.0%/);
  expect(html).toContain('Wolf · per minion · limit 7');
  expect(html).toContain('data-stat="MainMinionDPS">150');
});

test('a new class picks its strongest base attribute when metadata arrives and retains an explicit override', () => {
  session.character = { level: 1, class_name: 'Wayfarer', ascendancy_name: '' };
  render();
  session.treeMeta = { tree: 'Default', classes: [{ name: 'Wayfarer', base_str: 30, base_dex: 7, base_int: 7 }] };
  render();
  expect(planner.attribute).toBe('str');
  planner.setAttribute('int');
  session.treeMeta = { tree: 'Default', classes: [{ name: 'Wayfarer', base_str: 7, base_dex: 40, base_int: 7 }] };
  render();
  expect(planner.attribute).toBe('int');
});
