import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import type { BuildSession } from '../../hooks/useBuildSession';
import type { PassiveJewelState, PassiveNode } from '../../api/types';
import type { ObjectiveState } from '../shared/OptimizerControls';
import { TreeOptimizer, useTreeOptimizer, type TreeOptimizerState } from './TreeOptimizer';

const ui = vi.hoisted(() => ({ plan: vi.fn(), jewel: vi.fn(), confirm: vi.fn(), goal: { preset: 'dps', cStat: '', cMin: '', cMax: '' } as ObjectiveState }));
const hooks = await vi.hoisted(async () => {
  const { createHookHarness } = await import('../../testing/hookHarness');
  return createHookHarness({ refs: true, memo: 'deps', callback: 'deps',
    effects: { comparison: 'objectIs', cleanup: 'previous' }, layoutEffect: 'effect' });
});
vi.mock('react', async original => hooks.mockReact(await original<typeof import('react')>()));

vi.mock('../../app/jobs/useJobBeacon', () => ({ useJobBeacon: () => {} }));
vi.mock('../shared/AppDialogs', () => ({ useAppDialogs: () => ({ confirm: ui.confirm }) }));
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
let pointSummary: TreeOptimizerState['pointSummary'];
let active: boolean;
const setAllocatedNodes = vi.fn();
const focusPlanner = { nonce: 1 }, onPreview = vi.fn(), nodeLabel = () => 'Minion power';
function Harness() {
  planner = useTreeOptimizer({ session, nodes, pointSummary, lang: 'en-US', active, focusPlanner, onPreview, nodeLabel });
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
  hooks.reset(); ui.plan.mockReset(); ui.jewel.mockClear(); ui.confirm.mockReset(); setAllocatedNodes.mockClear();
  ui.goal = { preset: 'dps', cStat: '', cMin: '', cMax: '' };
  pointSummary = { used: 1, max: 113, weapon: [0, 0] };
  active = true;
  vi.useFakeTimers(); vi.stubGlobal('window', { setTimeout, clearTimeout });
  const request = { allocated_nodes: [1] };
  session = { currentRequest: () => request, allocatedNodes: [1], attributeChoices: {}, jewels: [], setAllocatedNodes,
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

test('both searches share the available point cap without overwriting the requested budget', async () => {
  pointSummary = { ...pointSummary, used: 112 };
  expect(render()).toContain('Unspent points: 1');
  expect(planner.mode).toBe('allocate');
  expect(planner.pointLimit).toBe(1);
  planner.run(); render(); await vi.advanceTimersByTimeAsync(350); render();
  expect(ui.plan).toHaveBeenLastCalledWith(expect.objectContaining({ mode: 'allocate', points: 1 }));
  expect(ui.jewel).toHaveBeenLastCalledWith(expect.objectContaining({ mode: 'allocate', points: 1, searchAllowed: true }));
  planner.setMode('future'); render();
  planner.setPoints(7); render();
  active = false; render(); active = true; render();
  expect(planner.mode).toBe('future');
  expect(planner.points).toBe(7);
  planner.setMode('allocate'); render();
  expect(planner.points).toBe(1);
  pointSummary = { ...pointSummary, used: 100 }; render();
  expect(planner.points).toBe(7);
});

test('a full tree with refundable points defaults to reallocation', () => {
  pointSummary = { ...pointSummary, used: 113 };
  session.allocatedNodes = [1, 2];
  session.currentRequest = () => ({ allocated_nodes: [1, 2] });
  render();
  expect(planner.mode).toBe('reallocate');
  expect(planner.searchAllowed).toBe(true);
});

test('future apply rejects a receipt when the build changes during confirmation', async () => {
  pointSummary = { ...pointSummary, used: 113 };
  expect(render()).toContain('Choose “Plan with future points” to look ahead.');
  expect(planner.mode).toBe('allocate');
  planner.run(); render(); await vi.advanceTimersByTimeAsync(350);
  expect(ui.plan).not.toHaveBeenCalled();
  expect(ui.jewel).toHaveBeenLastCalledWith(expect.objectContaining({ searchAllowed: false }));
  const plan = { allocate: [2], deallocate: [], target: 2, stats: { CombinedDPS: 150 } };
  ui.plan.mockResolvedValue({ baseline: { CombinedDPS: 100 }, plans: [plan],
    evaluated: 1, limited: false, unmodeled: 0, baselineUnsupported: [], excluded: [] });
  planner.setMode('future'); render();
  planner.run(); render(); await vi.advanceTimersByTimeAsync(350);
  const html = render();
  expect(html).toContain('Extra passive points needed: 1');
  expect(html).toContain('Apply future plan');
  expect(ui.plan).toHaveBeenLastCalledWith(expect.objectContaining({ mode: 'allocate', points: 3 }));
  let confirm!: (answer: boolean) => void;
  ui.confirm.mockImplementation(() => new Promise<boolean>(resolve => { confirm = resolve; }));
  const apply = planner.apply(plan);
  expect(ui.confirm).toHaveBeenCalledWith(expect.stringContaining('budget by 1.'), 'Apply over budget');
  expect(setAllocatedNodes).not.toHaveBeenCalled();
  session.stateVersion++; render();
  confirm(true); await apply;
  expect(setAllocatedNodes).not.toHaveBeenCalled();
});
