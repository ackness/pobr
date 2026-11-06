import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import type { BuildSession } from '../../hooks/useBuildSession';
import type { PassiveNode } from '../../api/types';
import type { ObjectiveState } from '../shared/OptimizerControls';
import { TreeOptimizer, useTreeOptimizer, type TreeOptimizerState } from './TreeOptimizer';

const ui = vi.hoisted(() => ({ cursor: 0, dirty: false, slots: [] as unknown[], effects: [] as (() => void)[],
  plan: vi.fn(), jewel: vi.fn(), goal: { preset: 'dps', cStat: '', cMin: '', cMax: '' } as ObjectiveState }));
vi.mock('react', async original => {
  const memo = (factory: () => unknown, deps?: unknown[]) => {
    const index = ui.cursor++;
    const previous = ui.slots[index] as { deps?: unknown[]; value: unknown } | undefined;
    if (!previous || !deps || deps.some((value, i) => !Object.is(value, previous.deps?.[i]))) {
      ui.slots[index] = { deps, value: factory() };
    }
    return (ui.slots[index] as { value: unknown }).value;
  };
  const effect = (callback: () => void | (() => void), deps?: unknown[]) => {
    const index = ui.cursor++;
    const previous = ui.slots[index] as { deps?: unknown[]; cleanup?: () => void } | undefined;
    if (!previous || !deps || deps.some((value, i) => !Object.is(value, previous.deps?.[i]))) {
      const entry = { deps, cleanup: undefined as (() => void) | undefined };
      ui.slots[index] = entry;
      ui.effects.push(() => { previous?.cleanup?.(); entry.cleanup = callback() || undefined; });
    }
  };
  return { ...await original<typeof import('react')>(),
    useState: (initial: unknown) => {
      const index = ui.cursor++;
      if (!(index in ui.slots)) ui.slots[index] = typeof initial === 'function' ? initial() : initial;
      return [ui.slots[index], (value: unknown) => { ui.slots[index] = typeof value === 'function' ? value(ui.slots[index]) : value; ui.dirty = true; }];
    },
    useRef: (initial: unknown) => {
      const index = ui.cursor++;
      if (!(index in ui.slots)) ui.slots[index] = { current: initial };
      return ui.slots[index];
    },
    useMemo: memo, useCallback: (callback: unknown, deps?: unknown[]) => memo(() => callback, deps),
    useEffect: effect, useLayoutEffect: effect,
  };
});

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
let session: BuildSession;
let planner: TreeOptimizerState;
const focusPlanner = { nonce: 1 }, onPreview = vi.fn(), nodeLabel = () => 'Minion power';
function Harness() {
  planner = useTreeOptimizer({ session, nodes, lang: 'en-US', active: true, focusPlanner, onPreview, nodeLabel });
  return <TreeOptimizer planner={planner} />;
}
function render() {
  for (let count = 0; count < 10; count++) {
    ui.cursor = 0; ui.dirty = false;
    const html = renderToStaticMarkup(<Harness />);
    ui.effects.splice(0).forEach(effect => effect());
    if (!ui.dirty) return html;
  }
  throw new Error('Render did not settle');
}
beforeEach(() => {
  ui.slots = []; ui.effects = []; ui.plan.mockReset(); ui.jewel.mockClear();
  ui.goal = { preset: 'dps', cStat: '', cMin: '', cMax: '' };
  vi.useFakeTimers(); vi.stubGlobal('window', { setTimeout, clearTimeout });
  const request = { allocated_nodes: [1] };
  session = { currentRequest: () => request, allocatedNodes: [1], attributeChoices: {}, jewels: [],
    character: { class_name: 'Witch' }, activeWeaponSet: 1, stateVersion: 1, calc: { stats: [] },
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
  session.calc = { stats: [{ id: 'ActiveMinionLimit', value: 7 }], main_minion_index: 2,
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
