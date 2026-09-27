import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import type { JewelBase } from '../api/backend';
import type { CalculateBuildRequest, CalculateBuildResponse, PassiveJewelState, PassiveNode } from '../api/types';
import type { EvaluateOptions } from './optimize';
import { planJewelPassiveUpgrades, type JewelPassivePlannerOptions } from './jewelPassivePlanner';

const node = (skill: number, connections: number[] = [], extra: Partial<PassiveNode> = {}): PassiveNode => ({
  id: String(skill), skill, connections, kind: 'normal', name: skill === 1 ? 'WITCH' : `Node ${skill}`, ...extra,
});
const text = (name: string) => `Rarity: UNIQUE\n${name}\nRuby\nLimited to: 1`;
const emptyEffects = (): PassiveJewelState => ({ class_starts: { witch: 1 }, allocation_grants: [], nodes: {}, conquered: [], unresolved: [], rings: [], warnings: [] });
const catalog = { bases: [{ name: 'Ruby', category: 'jewel', level: 1, tags: [], implicits: [] }], mods: [] };
function setup(nodes: PassiveNode[], request: CalculateBuildRequest, score: (request: CalculateBuildRequest) => number,
  effects: (request: CalculateBuildRequest) => PassiveJewelState = emptyEffects) {
  const seen: CalculateBuildRequest[] = [];
  const calculateBuild = async (input: CalculateBuildRequest): Promise<CalculateBuildResponse> => {
    seen.push(input);
    return { stats: [{ id: 'TotalDPS', value: score(input), category: 'Offence' }], unsupported_modifiers: [],
      breakdowns: {}, main_skill: null, item_errors: [], tree_effects: effects(input) };
  };
  const evaluate = async (options: EvaluateOptions) => ({ baseline: { TotalDPS: score(options.request) }, aborted: false,
    results: await Promise.all(options.variants.map(async (variant, index) => {
      const input = { ...options.request, allocated_nodes: [...(options.request.allocated_nodes ?? [])
        .filter(id => !variant.deallocate_nodes?.includes(id)), ...(variant.allocate_nodes ?? [])] };
      return { index, label: null, error: null, stats: { TotalDPS: score(input) }, unsupported: [] };
    })),
  });
  const options: JewelPassivePlannerOptions = { nodes, request: { character: { level: 90 }, ...request }, className: 'Witch',
    points: 3, mode: 'allocate', objective: { stat: 'TotalDPS', constraints: [] }, candidates: [],
    calculateBuild, evaluate, catalog, translate: async lines => lines };
  return { options, seen };
}
const has = (request: CalculateBuildRequest, id: number) => request.allocated_nodes?.includes(id);
const wearing = (request: CalculateBuildRequest, name: string) => request.jewels?.some(jewel => jewel.text.includes(`\n${name}\n`));
const candidate = (name = 'Synergy', socket_node = 2) => ({ id: name, label: name, jewel: { socket_node, text: text(name) } });

describe('joint jewel and passive planning', () => {
  it('finds a weak jewel with interacting radius nodes and compares every outcome to the original build', async () => {
    const nodes = [node(1, [2, 3]), node(2, [], { kind: 'jewel_socket' }), node(3, [4, 5]), node(4), node(5)];
    const { options, seen } = setup(nodes, { allocated_nodes: [2], tree_version: '0.4.0', attribute_choices: { '3': 'int' } },
      request => 100 + (wearing(request, 'Synergy') ? -5 + (has(request, 4) && has(request, 5) ? 100 : 0) : has(request, 3) ? 2 : 0));
    const result = await planJewelPassiveUpgrades({ ...options, candidates: [candidate()] });
    expect(result.baseline.TotalDPS).toBe(100);
    expect(result.plans[0]).toMatchObject({ candidateId: 'Synergy', stats: { TotalDPS: 195 }, deallocate: [], attributeChoices: { '3': 'int' } });
    expect(result.plans[0].allocatedNodes.sort()).toEqual([2, 3, 4, 5]);
    expect(result.plans.some(plan => plan.candidateId === null)).toBe(true);
    expect(seen.every(request => request.tree_version === '0.4.0')).toBe(true);
    expect(seen.at(-1)?.allocated_nodes).toEqual(result.plans.find(plan => plan.candidateId === 'Synergy')?.allocatedNodes);
  });

  it('charges an unallocated socket route and uses the candidate grant rather than baseline geometry', async () => {
    const nodes = [node(1, [2, 8]), node(2, [3]), node(3, [], { kind: 'jewel_socket' }), node(8, [9]), node(9)];
    const { options } = setup(nodes, {}, request => 100 + (wearing(request, 'Synergy') && has(request, 9) ? 80 : 0), request => ({
      ...emptyEffects(), allocation_grants: wearing(request, 'Synergy') && has(request, 3) ? [{ source: 3, nodes: [9], roots: [] }] : [],
    }));
    const result = await planJewelPassiveUpgrades({ ...options, candidates: [candidate('Synergy', 3)] });
    expect(result.plans[0].allocate.sort()).toEqual([2, 3, 9]);
    const short = await planJewelPassiveUpgrades({ ...options, points: 2, candidates: [candidate('Synergy', 3)] });
    expect(short.plans).toEqual([]);
  });

  it('includes a jewel-only improvement even with no useful passive route', async () => {
    const { options } = setup([node(1, [2]), node(2, [], { kind: 'jewel_socket' })], { allocated_nodes: [2] },
      request => wearing(request, 'Synergy') ? 130 : 100);
    const result = await planJewelPassiveUpgrades({ ...options, candidates: [candidate()] });
    expect(result.plans[0]).toMatchObject({ candidateId: 'Synergy', allocate: [], deallocate: [], allocatedNodes: [2] });
  });

  it('rejects grant revocation in allocate mode and charges forced dependent refunds in refund mode', async () => {
    const nodes = [node(1, [2, 3, 6]), node(2, [], { kind: 'jewel_socket' }), node(3, [4]), node(4, [5]), node(5), node(6)];
    const { options } = setup(nodes, { allocated_nodes: [2, 5], jewels: [{ socket_node: 2, text: text('Old') }] },
      request => 100 + (wearing(request, 'New') ? 5 : 0) + (has(request, 6) ? 30 : 0), request => ({ ...emptyEffects(),
        allocation_grants: wearing(request, 'Old') ? [{ source: 2, nodes: [5], roots: [] }] : [] }));
    const allocation = await planJewelPassiveUpgrades({ ...options, points: 1, candidates: [candidate('New')] });
    expect(allocation.plans.every(plan => plan.candidateId === null)).toBe(true);
    expect(allocation.diagnostics.join(' ')).toContain('disconnect');
    const refund = await planJewelPassiveUpgrades({ ...options, mode: 'reallocate', points: 1, candidates: [candidate('New')] });
    expect(refund.plans[0]).toMatchObject({ candidateId: 'New', deallocate: [5], allocate: [6] });
    expect(refund.plans.every(plan => plan.allocate.length <= plan.deallocate.length && plan.deallocate.length <= 1)).toBe(true);
  });

  it('searches voluntary refunds while retaining filled sockets', async () => {
    const nodes = [node(1, [2, 3, 4]), node(2, [], { kind: 'jewel_socket' }), node(3), node(4, [5]), node(5)];
    const { options } = setup(nodes, { allocated_nodes: [2, 3] }, request => 100 + (wearing(request, 'Synergy') && has(request, 4) ? 40 : 0));
    const result = await planJewelPassiveUpgrades({ ...options, points: 1, mode: 'reallocate', candidates: [candidate()] });
    expect(result.plans[0]).toMatchObject({ allocate: [4], deallocate: [3], allocatedNodes: [2, 4] });
  });

  it('uses known transformed-node outcomes but protects unresolved seeds and reports new warnings', async () => {
    const nodes = [node(1, [2, 3, 4]), node(2, [], { kind: 'jewel_socket' }), node(3), node(4)];
    const { options } = setup(nodes, { allocated_nodes: [2] }, request => 100 + (wearing(request, 'Synergy') ? (has(request, 3) ? 30 : 0) + (has(request, 4) ? 100 : 0) : 0),
      request => ({ ...emptyEffects(), nodes: { '3': { name: 'Transformed', stats: ['30% increased Damage'], replace: true } },
        unresolved: [4], warnings: wearing(request, 'Unsupported') ? ['Missing seed data'] : [] }));
    const result = await planJewelPassiveUpgrades({ ...options, candidates: [candidate(), candidate('Unsupported')] });
    expect(result.plans[0].allocate).toEqual([3]);
    expect(result.plans.some(plan => plan.allocate.includes(4))).toBe(false);
    expect(result.diagnostics).toContain('Unsupported: Missing seed data');
    expect(result.unmodeled).toBeGreaterThan(0);
  });

  it('rejects invalid item types and level requirements without silently calculating them', async () => {
    const { options } = setup([node(1, [2]), node(2, [], { kind: 'jewel_socket' })], { allocated_nodes: [2] }, () => 100);
    const result = await planJewelPassiveUpgrades({ ...options, candidates: [
      { ...candidate(), jewel: { socket_node: 2, text: `${text('Synergy')}\nLevelReq: 99` } },
      { ...candidate('Wrong'), jewel: { socket_node: 2, text: 'Rarity: NORMAL\nUnknown' } },
    ] });
    expect(result.diagnostics.join(' ')).toContain('item-level');
    expect(result.diagnostics.join(' ')).toContain('unknown-base');
    expect(result.plans).toEqual([]);
  });

  it('never returns partial plans on cancellation and rejects weapon-set allocations', async () => {
    const { options } = setup([node(1, [2]), node(2, [], { kind: 'jewel_socket' })], { allocated_nodes: [2] }, request => wearing(request, 'Synergy') ? 130 : 100);
    const controller = new AbortController();
    const result = await planJewelPassiveUpgrades({ ...options, candidates: [candidate()], signal: controller.signal,
      evaluate: async values => { const response = await options.evaluate!(values); controller.abort(); return response; } });
    expect(result.aborted).toBe(true);
    expect(result.plans).toEqual([]);
    await expect(planJewelPassiveUpgrades({ ...options, exclusiveNodes: [9] })).rejects.toThrow('Weapon-exclusive');
  });

  it('rechecks final topology and unsupported effects before accepting evaluated improvements', async () => {
    const { options } = setup([node(1, [2, 3]), node(2, [], { kind: 'jewel_socket' }), node(3)], { allocated_nodes: [2] }, request => has(request, 3) ? 200 : 100);
    const calculate = options.calculateBuild!;
    const result = await planJewelPassiveUpgrades({ ...options, calculateBuild: async request => ({ ...await calculate(request),
      unsupported_modifiers: has(request, 3) ? ['Unmodeled transformation'] : [] }) });
    expect(result.plans).toEqual([]);
    expect(result.diagnostics.join(' ')).toContain('Unmodeled transformation');
  });

  it('rejects stale grant geometry after an exact final recalculation', async () => {
    const nodes = [node(1, [2, 3]), node(2, [], { kind: 'jewel_socket' }), node(3, [4]), node(4, [5]), node(5)];
    const { options } = setup(nodes, { allocated_nodes: [2] }, request => has(request, 5) ? 200 : 100,
      request => ({ ...emptyEffects(), allocation_grants: wearing(request, 'Synergy') && !has(request, 5)
        ? [{ source: 2, nodes: [5], roots: [] }] : [] }));
    const result = await planJewelPassiveUpgrades({ ...options, points: 1, candidates: [candidate()] });
    expect(result.plans).toEqual([]);
  });

  it('rejects limited duplicate uniques and candidate calculation errors while preserving other results', async () => {
    const { options } = setup([node(1, [2, 3]), node(2, [], { kind: 'jewel_socket' }), node(3, [], { kind: 'jewel_socket' })],
      { allocated_nodes: [2, 3], jewels: [{ socket_node: 3, text: text('Synergy') }] }, request => wearing(request, 'Good') ? 120 : 100);
    const calculate = options.calculateBuild!;
    const result = await planJewelPassiveUpgrades({ ...options, candidates: [candidate(), candidate('Error'), candidate('Good')],
      calculateBuild: request => {
        if (wearing(request, 'Error')) throw new Error('Synthetic calculation error');
        return calculate(request);
      } });
    expect(result.plans[0].candidateId).toBe('Good');
    expect(result.diagnostics).toContain('Synergy: Jewel limit exceeded');
    expect(result.diagnostics).toContain('Error: Synthetic calculation error');
  });

  it('validates unique-only Diamond against authoritative bases without widening the crafting catalog', async () => {
    const version = readFileSync(new URL('../../../data/CURRENT', import.meta.url), 'utf8').trim();
    const bases = JSON.parse(readFileSync(new URL(`../../../data/${version}/base/base_items.json`, import.meta.url), 'utf8')) as (JewelBase & { item_class: string })[];
    const trade = JSON.parse(readFileSync(new URL(`../../../data/${version}/overlay/trade_catalog.json`, import.meta.url), 'utf8')) as { bases: { name: string }[] };
    const diamond = bases.find(base => base.item_class === 'Jewel' && base.name === 'Diamond')!;
    expect(diamond).toBeDefined();
    expect(trade.bases.some(base => base.name === 'Diamond')).toBe(false);
    const { options } = setup([node(1, [2]), node(2, [], { kind: 'jewel_socket' })], { allocated_nodes: [2] }, request => wearing(request, 'Synergy') ? 130 : 100);
    const diamondCandidate = { ...candidate(), jewel: { socket_node: 2, text: text('Synergy').replace('Ruby', 'Diamond') } };
    const result = await planJewelPassiveUpgrades({ ...options, jewelBases: [diamond], candidates: [diamondCandidate] });
    expect(result.plans[0].candidateId).toBe('Synergy');
    expect(result.plans[0].jewels[0].text).toContain('Diamond');
    expect(options.catalog!.bases.some(base => base.name === 'Diamond')).toBe(false);
    const lowLevel = await planJewelPassiveUpgrades({ ...options, request: { ...options.request, character: { level: diamond.drop_level - 1 } },
      jewelBases: [diamond], candidates: [diamondCandidate] });
    expect(lowLevel.diagnostics).toContain('Synergy: item-level');
    const unknown = await planJewelPassiveUpgrades({ ...options, jewelBases: [diamond],
      candidates: [{ ...diamondCandidate, jewel: { socket_node: 2, text: diamondCandidate.jewel.text.replace('Diamond', 'Unknown Jewel') } }] });
    expect(unknown.diagnostics).toContain('Synergy: unknown-base');
  });

  it('rejects ambiguous IDs and non-finite baseline or candidate values', async () => {
    const { options } = setup([node(1, [2, 3]), node(2, [], { kind: 'jewel_socket' }), node(3)], { allocated_nodes: [2] }, () => 100);
    await expect(planJewelPassiveUpgrades({ ...options, candidates: [candidate(), candidate()] })).rejects.toThrow('IDs must be unique');
    const calculate = options.calculateBuild!;
    await expect(planJewelPassiveUpgrades({ ...options, calculateBuild: async request => ({ ...await calculate(request),
      stats: [{ id: 'TotalDPS', value: NaN, category: 'Offence' }] }) })).rejects.toThrow('non-finite');
    const evaluate = options.evaluate!;
    const result = await planJewelPassiveUpgrades({ ...options, evaluate: async input => ({ ...await evaluate(input),
      results: [{ index: 0, label: null, error: null, stats: { TotalDPS: Infinity } }] }) });
    expect(result.plans).toEqual([]);
    expect(result.diagnostics.join(' ')).toContain('non-finite');
  });
});
