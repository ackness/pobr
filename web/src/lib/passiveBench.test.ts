/** Opt-in real-engine experiment; ordinary unit tests do not require generated WASM. */
import { beforeAll, describe, expect, test, vi } from 'vitest';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { env } from 'node:process';
import type { AttributeChoice, CalculateBuildRequest, CalculateBuildResponse, OptimizeVariantsResponse, PassiveNode, VariantInput } from '../api/types';
import { compareObjectiveStats, evaluateVariants, scoreOf, type Objective } from './optimize';
import { connectedAllocation } from './passiveGraph';
import { passivePlanningContext, planPassiveUpgrades, withTravelAttributes, type PassivePlan, type PassivePlanningContext } from './passivePlanner';

const bridge = vi.hoisted(() => ({ optimize: undefined as undefined | ((input: string) => string) }));
vi.mock('../api/backend', () => ({ getBackend: async () => ({ optimizeVariants: async (request: unknown) =>
  JSON.parse(bridge.optimize!(JSON.stringify(request))) as OptimizeVariantsResponse }) }));

interface Result {
  scenario: string;
  baseline: Record<string, number>;
  oracleCandidates: number;
  plannerEvaluations: number;
  milliseconds: number;
  selected: PassivePlan;
  predicted: { dps: number; ehp: number; objective: number };
  applied: { dps: number; ehp: number; objective: number };
  oracle: { dps: number; ehp: number; objective: number };
  regretPercent: number;
  unmodeledSkipped: number;
}

const objective: Objective = { stat: 'TotalDPS', secondaryStat: 'TotalEHP', constraints: [] };
const neighborhood = [54447, 4739, 22419, 18407, 47555, 39886, 51184, 53697];
const existing = [4739, 22419, 18407];
const candidates = [47555, 39886, 51184, 53697];
const travelChoice: AttributeChoice = 'str';
const metrics = (stats: Record<string, number>) => ({ dps: stats.TotalDPS, ehp: stats.TotalEHP, objective: scoreOf(stats, objective) });

function allocationOracle(context: PassivePlanningContext, points: number): VariantInput[] {
  const variants: VariantInput[] = [];
  for (let mask = 1; mask < 2 ** candidates.length; mask += 1) {
    const allocate = candidates.filter((id, index) => mask & 1 << index && !context.allocated.has(id));
    if (allocate.length === 0 || allocate.length > points) continue;
    const final = new Set([...context.allocated, ...allocate]);
    if (connectedAllocation(context.graph, final, context.root).size !== final.size) continue;
    variants.push({ allocate_nodes: allocate });
  }
  return variants;
}

/** Reproduce the session's single atomic commit, including pruning refunded attribute choices. */
function applyPlan(request: CalculateBuildRequest, nodes: PassiveNode[], plan: PassivePlan): CalculateBuildRequest {
  const allocated = new Set((request.allocated_nodes ?? []).filter(id => !plan.deallocate.includes(id)));
  plan.allocate.forEach(id => allocated.add(id));
  const choices = Object.fromEntries(Object.entries(request.attribute_choices ?? {}).filter(([id]) => allocated.has(Number(id))));
  for (const id of plan.allocate) {
    if (nodes.find(node => node.skill === id)?.name === 'Attribute') choices[String(id)] = travelChoice;
  }
  return { ...request, allocated_nodes: [...allocated], attribute_choices: choices };
}

describe.skipIf(env.POBR_UPGRADE_BENCH !== '1')('real WASM passive planner verification', () => {
  let nodes: PassiveNode[];
  let allNodes: PassiveNode[];
  let calculate: (request: CalculateBuildRequest) => Record<string, number>;
  let version: string;
  let schema: number;
  let initializationMs: number;
  const report: Result[] = [];

  beforeAll(async () => {
    const start = performance.now();
    const modulePath = '../wasm/pkg/pobr_wasm.js';
    const wasm = await import(/* @vite-ignore */ modulePath);
    wasm.initSync({ module: await readFile(new URL('../wasm/pkg/pobr_wasm_bg.wasm', import.meta.url)) });
    const manifest = JSON.parse(await readFile(new URL('../../public/data/manifest.json', import.meta.url), 'utf8')) as { version: string; files: string[] };
    version = manifest.version;
    schema = wasm.schemaVersion();
    // The manifest supplies the file list; contents come from current repository data, including unlock metadata.
    const data = new URL(`../../../data/${version}/`, import.meta.url);
    for (const file of manifest.files) {
      const source = file.startsWith('overlay-common/') ? new URL(`../${file}`, data) : new URL(file, data);
      wasm.stageDataFile(file, await readFile(source, 'utf8'));
    }
    wasm.initStagedData();
    bridge.optimize = wasm.optimizeVariantsJson;
    calculate = request => {
      const response = JSON.parse(wasm.calculateBuildJson(JSON.stringify(request))) as CalculateBuildResponse;
      return Object.fromEntries(response.stats.map(stat => [stat.id, stat.value ?? 0]));
    };
    allNodes = JSON.parse(await readFile(new URL('base/passive_tree.json', data), 'utf8')) as PassiveNode[];
    nodes = neighborhood.map(id => allNodes.find(node => node.skill === id)!);
    expect(nodes.every(Boolean)).toBe(true);
    expect(nodes.find(node => node.skill === 47555)?.name).toBe('Attribute');
    expect(nodes.find(node => node.skill === 51184)?.name).toBe('Raw Power');
    initializationMs = performance.now() - start;
  }, 120000);

  test('matches the legal neighborhood oracle and the actual application of new points and refunds', async () => {
    const base: CalculateBuildRequest = {
      character: { level: 85, class_name: 'Sorceress' }, allocated_nodes: existing,
      attribute_choices: { '22419': 'int', '18407': 'int' },
      socket_groups: [{ enabled: true, gems: [{ skill_id: 'FireballPlayer', level: 16, quality: 0 }] }], main_socket_group: 0,
      extra_modifiers: ['+1000 to maximum Life', '+500 to maximum Energy Shield', '+55% to all Elemental Resistances'],
    };
    for (const mode of ['allocate', 'reallocate'] as const) {
      const request: CalculateBuildRequest = mode === 'allocate' ? base : {
        ...base, allocated_nodes: [...existing, 47555, 39886],
        attribute_choices: { ...base.attribute_choices, '47555': 'int', '39886': 'int' },
      };
      const context = passivePlanningContext(nodes, request.allocated_nodes!, 'Sorceress');
      const points = mode === 'allocate' ? 3 : 1;
      const prepared = withTravelAttributes(request, nodes, travelChoice);
      const baseline = calculate(request);
      // Prefilling future attributes must never change an existing attribute node or the baseline.
      expect(calculate(prepared).TotalDPS).toBeCloseTo(baseline.TotalDPS, 8);
      expect(calculate(prepared).TotalEHP).toBeCloseTo(baseline.TotalEHP, 8);
      expect(baseline.TotalDPS).toBeGreaterThan(0);
      expect(baseline.TotalEHP).toBeGreaterThan(0);
      const start = performance.now();
      const planned = await planPassiveUpgrades({ request: prepared, context, points, mode, objective });
      const milliseconds = performance.now() - start;
      expect(planned.plans.length).toBeGreaterThan(0);
      expect(planned.evaluated).toBeLessThanOrEqual(512);
      const selected = planned.plans[0];
      const afterRequest = applyPlan(request, nodes, selected);
      const final = new Set(afterRequest.allocated_nodes);
      expect(connectedAllocation(context.graph, final, context.root).size).toBe(final.size);
      if (mode === 'allocate') {
        expect(selected.allocate).toContain(47555);
        expect(afterRequest.attribute_choices?.['47555']).toBe('str');
        expect(afterRequest.attribute_choices?.['22419']).toBe('int');
      } else {
        expect(selected.deallocate).toContain(39886);
        expect(selected.allocate).toContain(51184);
        expect(afterRequest.attribute_choices?.['39886']).toBeUndefined();
        expect(afterRequest.attribute_choices?.['47555']).toBe('int');
        expect(selected.allocate.length).toBeLessThanOrEqual(selected.deallocate.length);
      }
      const applied = calculate(afterRequest);
      expect(applied.TotalDPS).toBeCloseTo(selected.stats.TotalDPS, 8);
      expect(applied.TotalEHP).toBeCloseTo(selected.stats.TotalEHP, 8);
      let variants: VariantInput[];
      if (mode === 'allocate') {
        variants = allocationOracle(context, points);
        expect(variants.length).toBe(7);
      } else {
        // Every legal one-point removal/replacement in the same small neighborhood.
        variants = request.allocated_nodes!.flatMap(removed => candidates.filter(id => !context.allocated.has(id)).flatMap(added => {
          const final = new Set(request.allocated_nodes!.filter(id => id !== removed).concat(added));
          return connectedAllocation(context.graph, final, context.root).size === final.size
            ? [{ allocate_nodes: [added], deallocate_nodes: [removed] }] : [];
        }));
      }
      expect(variants.length).toBeGreaterThan(0);
      expect(variants.length).toBeLessThanOrEqual(30);
      const truth = await evaluateVariants({ request: prepared, variants });
      expect(truth.results.every(row => !row.error)).toBe(true);
      const oracle = [...truth.results].sort((a, b) => compareObjectiveStats(a.stats, b.stats, objective))[0];
      expect(scoreOf(selected.stats, objective)).toBeCloseTo(scoreOf(oracle.stats, objective), 8);
      const result: Result = { scenario: `sorceress-fireball-${mode}`, baseline,
        oracleCandidates: variants.length, plannerEvaluations: planned.evaluated, milliseconds,
        selected: { allocate: selected.allocate, deallocate: selected.deallocate, target: selected.target },
        predicted: metrics(selected.stats), applied: metrics(applied), oracle: metrics(oracle.stats),
        regretPercent: Math.max(0, (scoreOf(oracle.stats, objective) - scoreOf(selected.stats, objective)) / scoreOf(oracle.stats, objective) * 100),
        unmodeledSkipped: planned.unmodeled };
      report.push(result);
      console.table([{ scenario: result.scenario, candidates: result.oracleCandidates, evaluations: result.plannerEvaluations,
        dps: result.predicted.dps, ehp: result.predicted.ehp, regretPercent: result.regretPercent, applyDpsDifference: applied.TotalDPS - selected.stats.TotalDPS,
        applyEhpDifference: applied.TotalEHP - selected.stats.TotalEHP }]);
    }
    const broadContext = passivePlanningContext(allNodes, existing, 'Sorceress');
    const controller = new AbortController();
    const cancelled = await planPassiveUpgrades({ request: withTravelAttributes(base, allNodes, travelChoice), context: broadContext,
      points: 8, mode: 'allocate', objective, signal: controller.signal,
      onProgress: done => { if (done > 0) controller.abort(); } });
    expect(cancelled.plans).toEqual([]);
    expect(cancelled.evaluated).toBeGreaterThan(0);
    expect(cancelled.evaluated).toBeLessThanOrEqual(16);
    const broad = await planPassiveUpgrades({ request: withTravelAttributes(base, allNodes, travelChoice), context: broadContext,
      points: 8, mode: 'allocate', objective });
    expect(broad.evaluated).toBeGreaterThan(cancelled.evaluated);
    expect(broad.evaluated).toBeLessThanOrEqual(512);
    const cache = new URL('../../../.cache/', import.meta.url);
    await mkdir(cache, { recursive: true });
    await writeFile(new URL('passive-algorithm-bench.json', cache), JSON.stringify({ version, schema, initializationMs,
      methodology: 'Real WASM; current repository data; synthetic level-85 Fireball Sorceress; fixed real eight-node neighborhood; all legal allocations up to three points and all legal one-point swaps. New travel attributes use Strength; allocated Intelligence choices are preserved. Every chosen plan is independently applied and recalculated. This bounded neighborhood is not a game-wide optimum or timing benchmark. Broad real-tree search is also evaluated through completion and checked against the 512-evaluation cap, and cancellation is checked at the first batch. The deterministic 300-frontier unit fixture separately exercises the worst-case cap.',
      results: report, cancellation: { evaluated: cancelled.evaluated, recommendations: cancelled.plans.length }, broadSearch: { evaluated: broad.evaluated, limited: broad.limited, unmodeledSkipped: broad.unmodeled }, worstCaseEvaluationCap: 512 }, null, 2), 'utf8');
  }, 120000);
});
