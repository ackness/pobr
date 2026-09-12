import { describe, expect, it } from 'vitest';
import type { CalculateBuildRequest, PassiveNode } from '../api/types';
import { connectedAllocation } from './passiveGraph';
import { scoreOf, type EvaluateOptions, type Objective } from './optimize';
import { allocationPaths, passivePlanAllocation, passivePlanningContext, planPassiveUpgrades, refundableBranches, withTravelAttributes, type PassivePlanningContext } from './passivePlanner';

const node = (skill: number, connections: number[] = [], extra: Partial<PassiveNode> = {}): PassiveNode => ({
  skill, id: String(skill), name: skill === 1 ? 'WITCH' : `Node ${skill}`, kind: 'normal', connections, ...extra,
});
const objective: Objective = { stat: 'TotalDPS', secondaryStat: 'TotalEHP', constraints: [] };
const stats = (dps: number, ehp = 100) => ({ TotalDPS: dps, TotalEHP: ehp, Life: 100 });
function evaluator(base: number[], calculate: (nodes: Set<number>) => Record<string, number>) {
  let evaluations = 0;
  const evaluate = async (options: EvaluateOptions) => {
    evaluations += options.variants.length;
    return {
      baseline: calculate(new Set(base)), aborted: false,
      results: options.variants.map((variant, index) => ({ index, label: null, error: null,
        stats: calculate(new Set([...base.filter(id => !variant.deallocate_nodes?.includes(id)), ...(variant.allocate_nodes ?? [])])),
      })),
    };
  };
  return { evaluate, count: () => evaluations };
}

/** Small-graph exhaustive oracle checks the heuristic against every legal point-budget allocation. */
function exhaustive(context: PassivePlanningContext, budget: number, calculate: (nodes: Set<number>) => Record<string, number>) {
  const available = [...context.byId.keys()].filter(id => id !== context.root && !context.allocated.has(id));
  let best = 0;
  let evaluations = 0;
  for (let mask = 1; mask < 2 ** available.length; mask += 1) {
    const selected = available.filter((_, index) => mask & (1 << index));
    if (selected.length > budget) continue;
    const allocated = new Set([...context.allocated, ...selected]);
    if (connectedAllocation(context.graph, allocated, context.root).size !== allocated.size) continue;
    best = Math.max(best, scoreOf(calculate(allocated), objective));
    evaluations += 1;
  }
  return { best, evaluations };
}

describe('passive upgrade planner', () => {
  it('preserves allocated weapon-set attribute choices while preparing new travel attributes', () => {
    const request = { allocated_nodes: [2], attribute_choices: { '2': 'str' as const } };
    const result = withTravelAttributes(request, [node(2, [], { name: 'Attribute' }), node(3, [], { name: 'Attribute' })], 'int');
    expect(result.attribute_choices).toEqual({ '2': 'str', '3': 'int' });
    expect(request.attribute_choices).toEqual({ '2': 'str' });
  });

  it('counts travel nodes, never invents disconnected or foreign class/mastery/ascendancy paths', () => {
    const nodes = [node(1, [2, 10, 11]), node(2, [3]), node(3), node(8),
      node(10, [12], { name: 'RANGER' }), node(12), node(11, [13], { kind: 'mastery' }), node(13),
      node(20, [21], { ascendancy_id: 'Test' }), node(21, [], { ascendancy_id: 'Test' })];
    const context = passivePlanningContext(nodes, [], 'Sorceress');
    expect(allocationPaths(context, context.allocated, 1).map(plan => plan.allocate)).toEqual([[2]]);
    expect(allocationPaths(context, context.allocated, 2).map(plan => plan.allocate)).toEqual([[2], [2, 3]]);
    expect(allocationPaths(context, context.allocated, 8).some(plan => [8, 10, 11, 12, 13, 20, 21].includes(plan.target))).toBe(false);
  });

  it('enforces ascendancy and every prerequisite on gated main-tree nodes', () => {
    const nodes = [node(1, [2, 4]), node(2, [3]), node(3),
      node(4, [], { unlock_constraint: { ascendancy: 'Druid1', nodes: [2, 3] } })];
    const wrongAscendancy = passivePlanningContext(nodes, [2, 3], 'Sorceress', [], [], 'Witch1');
    expect(allocationPaths(wrongAscendancy, wrongAscendancy.allocated, 3).some(plan => plan.target === 4)).toBe(false);
    const missingPrerequisite = passivePlanningContext(nodes, [2], 'Sorceress', [], [], 'Druid1');
    expect(allocationPaths(missingPrerequisite, missingPrerequisite.allocated, 3).some(plan => plan.target === 4)).toBe(false);
    const exclusivePrerequisite = passivePlanningContext(nodes, [2, 3], 'Sorceress', [3], [], 'Druid1');
    expect(exclusivePrerequisite.byId.has(4)).toBe(false);
    const unlocked = passivePlanningContext(nodes, [2, 3], 'Sorceress', [], [], 'Druid1');
    expect(allocationPaths(unlocked, unlocked.allocated, 1).map(plan => plan.target)).toContain(4);
    const allocated = passivePlanningContext(nodes, [2, 3, 4], 'Sorceress', [], [], 'Druid1');
    expect(refundableBranches(allocated, 3).some(plan => plan.deallocate.includes(2) || plan.deallocate.includes(3))).toBe(false);
  });

  it('revokes a candidate gate after its prerequisite is refunded without losing retained ascendancy gates', () => {
    const nodes = [node(1, [2, 3, 4]), node(2),
      node(3, [], { unlock_constraint: { nodes: [2] } }),
      node(4, [], { unlock_constraint: { nodes: [20] } }),
      node(20, [], { ascendancy_id: 'Witch1' })];
    const context = passivePlanningContext(nodes, [2, 20], 'Sorceress', [], [], 'Witch1');
    expect(allocationPaths(context, context.allocated, 1).map(plan => plan.target)).toEqual([3, 4]);
    expect(allocationPaths(context, new Set(), 1).map(plan => plan.target)).toEqual([2, 4]);
  });

  it('outperforms an illegal heatmap pick and matches the exhaustive legal-path optimum', async () => {
    const nodes = [node(1, [2, 4]), node(2, [3]), node(3), node(4, [5]), node(5)];
    const context = passivePlanningContext(nodes, [], 'Sorceress');
    const calculate = (ids: Set<number>) => stats(100 + (ids.has(3) ? 50 : 0) + (ids.has(5) ? 40 : 0));
    // The former top-single-node method selects [3, 5] for two points, but both require a travel node.
    expect(connectedAllocation(context.graph, new Set([3, 5]), 1).size).toBe(0);
    const oracle = exhaustive(context, 2, calculate);
    const engine = evaluator([], calculate);
    const result = await planPassiveUpgrades({ request: {}, context, points: 2, mode: 'allocate', objective, evaluate: engine.evaluate });
    expect(result.plans[0].allocate).toEqual([2, 3]);
    expect(scoreOf(result.plans[0].stats, objective)).toBe(oracle.best);
    expect(result.evaluated).toBe(engine.count());
    // One additional identity probe distinguishes existing unsupported effects from newly introduced ones.
    expect(result.evaluated).toBeLessThanOrEqual(oracle.evaluations + 1);
    expect(result.plans.every(plan => plan.allocate.length <= 2)).toBe(true);
  });

  it('finds interacting paths that have no individual DPS gain and shares travel cost', async () => {
    const nodes = [node(1, [2]), node(2, [3, 4]), node(3), node(4)];
    const context = passivePlanningContext(nodes, [], 'Sorceress');
    const calculate = (ids: Set<number>) => stats(ids.has(3) && ids.has(4) ? 200 : 100);
    const engine = evaluator([], calculate);
    const result = await planPassiveUpgrades({ request: {}, context, points: 3, mode: 'allocate', objective, evaluate: engine.evaluate });
    expect(result.plans[0].allocate).toEqual([2, 3, 4]);
    expect(scoreOf(result.plans[0].stats, objective)).toBe(exhaustive(context, 3, calculate).best);
  });

  it('reallocates within the old point budget and recomputes connectivity after refunding', async () => {
    const nodes = [node(1, [2, 4]), node(2, [3, 6]), node(3), node(4, [5]), node(5), node(6)];
    const context = passivePlanningContext(nodes, [2, 3], 'Sorceress');
    const calculate = (ids: Set<number>) => stats(100 + (ids.has(3) ? 5 : 0) + (ids.has(5) ? 30 : 0) + (ids.has(6) ? 10 : 0));
    const engine = evaluator([2, 3], calculate);
    const result = await planPassiveUpgrades({ request: {}, context, points: 2, mode: 'reallocate', objective, evaluate: engine.evaluate });
    expect(result.plans[0].deallocate).toEqual([2, 3]);
    expect(result.plans[0].allocate).toEqual([4, 5]);
    for (const plan of result.plans) {
      const final = new Set([...context.allocated].filter(id => !plan.deallocate.includes(id)).concat(plan.allocate));
      expect(plan.allocate.length).toBeLessThanOrEqual(plan.deallocate.length);
      expect(connectedAllocation(context.graph, final, context.root).size).toBe(final.size);
    }
  });

  it('keeps filled jewels and disables refunds for unexplained imports or weapon-set passives', () => {
    const nodes = [node(1, [2]), node(2, [3]), node(3, [], { kind: 'jewel_socket' }), node(8)];
    expect(refundableBranches(passivePlanningContext(nodes, [2, 3], 'Sorceress', [], [3]), 3)).toEqual([]);
    expect(passivePlanningContext(nodes, [8], 'Sorceress').canRefund).toBe(false);
    expect(passivePlanningContext(nodes, [2, 999], 'Sorceress').canRefund).toBe(false);
    expect(passivePlanningContext([...nodes, node(9, [], { kind: 'mastery' })], [2, 9], 'Sorceress').canRefund).toBe(false);
    const weapon = passivePlanningContext(nodes, [2, 3], 'Sorceress', [2]);
    expect(weapon.canRefund).toBe(false);
    expect(allocationPaths(weapon, new Set(), 3)).toEqual([]);
  });

  it('respects the survival floor and returns no partial recommendations after cancellation', async () => {
    const nodes = [node(1, [2, 3]), node(2), node(3)];
    const context = passivePlanningContext(nodes, [], 'Sorceress');
    const engine = evaluator([], ids => stats(ids.has(2) ? 200 : ids.has(3) ? 110 : 100, ids.has(2) ? 50 : 100));
    const result = await planPassiveUpgrades({ request: {}, context, points: 1, mode: 'allocate',
      objective: { ...objective, constraints: [{ stat: 'TotalEHP', min: 100 }] }, evaluate: engine.evaluate });
    expect(result.plans[0].allocate).toEqual([3]);
    const controller = new AbortController();
    const cancelled = await planPassiveUpgrades({ request: {}, context, points: 2, mode: 'allocate', objective,
      signal: controller.signal, evaluate: async options => { controller.abort(); return engine.evaluate(options); } });
    expect(cancelled.plans).toEqual([]);
  });

  it('cancels an edited tree request and recomputes travel cost and baseline from the latest allocation', async () => {
    const nodes = [node(1, [2]), node(2, [3], { name: 'Attribute' }), node(3)];
    const oldRequest: CalculateBuildRequest = { allocated_nodes: [2], attribute_choices: { '2': 'str' } };
    let finishOld!: () => void;
    const blocked = new Promise<void>(resolve => { finishOld = resolve; });
    const controller = new AbortController();
    const oldRun = planPassiveUpgrades({ request: oldRequest,
      context: passivePlanningContext(nodes, [2], 'Sorceress'), points: 2, mode: 'allocate', objective,
      signal: controller.signal, evaluate: async options => {
        await blocked;
        return evaluator(options.request.allocated_nodes ?? [], ids => stats(100 + (ids.has(2) ? 5 : 0) + (ids.has(3) ? 20 : 0))).evaluate(options);
      } });
    // A manual refund invalidates both the in-flight result and its one-point route to node 3.
    controller.abort();
    const request = withTravelAttributes({ allocated_nodes: [], attribute_choices: {} }, nodes, 'int');
    const context = passivePlanningContext(nodes, [], 'Sorceress');
    const latest = await planPassiveUpgrades({ request, context, points: 2, mode: 'allocate', objective,
      evaluate: options => evaluator(options.request.allocated_nodes ?? [], ids => stats(100 + (ids.has(2) ? 5 : 0) + (ids.has(3) ? 20 : 0))).evaluate(options) });
    finishOld();
    expect((await oldRun).plans).toEqual([]);
    expect(latest.baseline.TotalDPS).toBe(100);
    expect(latest.plans[0].allocate).toEqual([2, 3]);
    expect(passivePlanAllocation(request, context, latest.plans[0], 2, 'allocate')).toEqual({
      allocatedNodes: [2, 3], attributeChoices: { '2': 'int' },
    });
    // Never apply an old snapshot on top of the edited allocation.
    expect(passivePlanAllocation(oldRequest, context, { allocate: [3], deallocate: [], target: 3 }, 2, 'allocate')).toBeNull();
    expect(oldRequest.attribute_choices).toEqual({ '2': 'str' });
  });

  it('validates atomic application against connectivity, budgets, protected nodes and attribute choices', () => {
    const nodes = [node(1, [2, 4]), node(2, [3], { name: 'Attribute' }), node(3),
      node(4, [5], { name: 'Attribute' }), node(5, [], { kind: 'jewel_socket' }),
      node(20, [], { ascendancy_id: 'Witch1' })];
    const request = withTravelAttributes({ allocated_nodes: [2, 3, 20], attribute_choices: { '2': 'str' } }, nodes, 'int');
    const context = passivePlanningContext(nodes, [2, 3, 20], 'Sorceress');
    const plan = { allocate: [4], deallocate: [2, 3], target: 4 };
    expect(passivePlanAllocation(request, context, plan, 2, 'reallocate')).toEqual({
      allocatedNodes: [20, 4], attributeChoices: { '4': 'int' },
    });
    expect(passivePlanAllocation(request, context, plan, 1, 'reallocate')).toBeNull();
    expect(passivePlanAllocation(request, context, plan, 2, 'allocate')).toBeNull();
    expect(passivePlanAllocation(request, context, { ...plan, deallocate: [2] }, 2, 'reallocate')).toBeNull();
    expect(passivePlanAllocation(request, context, { ...plan, deallocate: [20] }, 2, 'reallocate')).toBeNull();
    expect(passivePlanAllocation(request, context, { allocate: [5], deallocate: [], target: 5 }, 2, 'allocate')).toBeNull();
    const jewelRequest = { allocated_nodes: [4, 5] };
    const protectedContext = passivePlanningContext(nodes, [4, 5], 'Sorceress', [], [5]);
    expect(passivePlanAllocation(jewelRequest, protectedContext, { allocate: [2, 3], deallocate: [4, 5], target: 3 }, 2, 'reallocate')).toBeNull();
    const weaponContext = passivePlanningContext(nodes, [2, 3, 20], 'Sorceress', [3]);
    expect(passivePlanAllocation(request, weaponContext, plan, 2, 'reallocate')).toBeNull();
  });

  it('does not evaluate an already cancelled search or report new gated gains after refunding', async () => {
    const nodes = [node(1, [2, 3]), node(2), node(3, [], { unlock_constraint: { nodes: [2] } })];
    const context = passivePlanningContext(nodes, [2], 'Sorceress');
    const engine = evaluator([2], ids => stats(100 + (ids.has(3) ? 200 : 0)));
    const controller = new AbortController();
    controller.abort();
    const cancelled = await planPassiveUpgrades({ request: {}, context, points: 2, mode: 'allocate', objective,
      signal: controller.signal, evaluate: engine.evaluate });
    expect(cancelled.evaluated).toBe(0);
    expect(engine.count()).toBe(0);
    const result = await planPassiveUpgrades({ request: {}, context, points: 1, mode: 'reallocate', objective, evaluate: engine.evaluate });
    expect(result.plans).toEqual([]);
  });

  it('excludes newly unmodeled passive effects while retaining baseline warnings', async () => {
    const context = passivePlanningContext([node(1, [2, 3]), node(2), node(3)], [], 'Sorceress');
    const engine = evaluator([], ids => stats(ids.has(2) ? 200 : ids.has(3) ? 120 : 100));
    const result = await planPassiveUpgrades({ request: {}, context, points: 1, mode: 'allocate', objective,
      evaluate: async options => {
        const response = await engine.evaluate(options);
        return { ...response, results: response.results.map(row => ({ ...row,
          unsupported: ['Also grants 102 Guard', ...(options.variants[row.index].allocate_nodes?.includes(2) ? ['Unknown drawback'] : [])],
        })) };
      },
    });
    expect(result.plans[0].allocate).toEqual([3]);
    expect(result.plans.some(plan => plan.allocate.includes(2))).toBe(false);
    expect(result.unmodeled).toBe(1);
  });

  it('caps broad searches deterministically', async () => {
    const nodes = [node(1, Array.from({ length: 300 }, (_, i) => i + 2)), ...Array.from({ length: 300 }, (_, i) => node(i + 2))];
    const context = passivePlanningContext(nodes, [], 'Sorceress');
    const engine = evaluator([], ids => stats(100 + ids.size));
    const result = await planPassiveUpgrades({ request: {}, context, points: 3, mode: 'allocate', objective, evaluate: engine.evaluate });
    expect(result.evaluated).toBeLessThanOrEqual(512);
    expect(result.limited).toBe(true);
  });

  it('caps refund searches and never increases the existing point count', async () => {
    const nodes = [node(1, Array.from({ length: 350 }, (_, i) => i + 2)), ...Array.from({ length: 350 }, (_, i) => node(i + 2))];
    const allocated = Array.from({ length: 80 }, (_, i) => i + 2);
    const context = passivePlanningContext(nodes, allocated, 'Sorceress');
    const engine = evaluator(allocated, ids => stats(100 + [...ids].reduce((sum, id) => sum + id, 0)));
    const result = await planPassiveUpgrades({ request: { allocated_nodes: allocated }, context, points: 99,
      mode: 'reallocate', objective, evaluate: engine.evaluate });
    expect(result.evaluated).toBeLessThanOrEqual(512);
    expect(result.limited).toBe(true);
    expect(result.plans.length).toBeGreaterThan(0);
    for (const plan of result.plans) {
      expect(plan.deallocate.length).toBeLessThanOrEqual(8);
      expect(passivePlanAllocation({ allocated_nodes: allocated }, context, plan, 8, 'reallocate')).not.toBeNull();
      expect(plan.allocate.length).toBeLessThanOrEqual(plan.deallocate.length);
    }
  });
});
