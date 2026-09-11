/** Opt-in real WASM comparison on fixed, small gem pools; ordinary tests do not load WASM. */
import { beforeAll, describe, expect, test, vi } from 'vitest';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { env } from 'node:process';
import type { CalculateBuildRequest, CalculateBuildResponse, GemInput, OptimizeVariantsResponse, SocketGroupInput } from '../api/types';
import { compareObjectiveStats, evaluateVariants, scoreOf, type Objective } from './optimize';
import { applySupportPlan, eligibleSupports, optimizeSupports, supportSetCompatible, supportVariant, usableSupportLevel, type SupportMetadata } from './supportOptimizer';
import { groupsForWeaponSet, switchWeapons, type WeaponSetState } from './weaponSets';
import type { TradeCatalog } from './tradeOptimizer';

const bridge = vi.hoisted(() => ({ optimize: undefined as undefined | ((input: string) => string) }));
vi.mock('../api/backend', () => ({ getBackend: async () => ({ optimizeVariants: async (request: unknown) =>
  JSON.parse(bridge.optimize!(JSON.stringify(request))) as OptimizeVariantsResponse }) }));

const fixtures = [
  { name: 'synthetic-ice-shot', skill: 'IceShotPlayer', other: 'FireballPlayer', className: 'Ranger', supports: [
    'SupportRapidAttacksPlayer', 'SupportAddedFireDamagePlayer', 'SupportAddedColdDamagePlayer',
    'SupportAddedLightningDamagePlayer', 'SupportIncreasedCriticalDamagePlayer', 'SupportElementalFocusPlayer',
  ] },
  { name: 'synthetic-fireball', skill: 'FireballPlayer', other: 'IceShotPlayer', className: 'Sorceress', supports: [
    'SupportRapidCastingPlayer', 'SupportControlledDestructionPlayer', 'SupportConsideredCastingPlayer',
    'SupportConcentratedAreaPlayer', 'SupportElementalFocusPlayer', 'SupportIncreasedCriticalDamagePlayer',
  ] },
] as const;

const statsOf = (response: CalculateBuildResponse) => Object.fromEntries(response.stats.map(stat => [stat.id, stat.value ?? 0]));
const gemInput = (gem: SupportMetadata): GemInput => ({ skill_id: gem.skill_id, level: usableSupportLevel(gem, 85), quality: 0 });
const sameStats = (actual: Record<string, number>, expected: Record<string, number>) => {
  expect(actual.TotalDPS).toBeCloseTo(expected.TotalDPS, 8);
  expect(actual.TotalEHP).toBeCloseTo(expected.TotalEHP, 8);
};

interface MethodResult {
  method: string;
  dps: number;
  ehp: number;
  objective: number;
  regretPercent: number;
  evaluations: number;
  milliseconds: number;
  supports: string[];
}

interface FixtureResult {
  build: string;
  candidates: string[];
  allCompatibleCandidates: number;
  legalCombinations: number;
  baseline: { dps: number; ehp: number };
  methods: MethodResult[];
  irrelevantAddedDamageGain: number;
  relevantAddedDamageGain: number;
  applyEquivalent: boolean;
  inactiveGroupPreserved: boolean;
  alternateWeaponSetPreserved: boolean;
  unsupported: string[];
}

describe.skipIf(env.POBR_UPGRADE_BENCH !== '1')('real WASM support planner comparison', () => {
  let catalog: SupportMetadata[];
  let version: string;
  let schema: number;
  let initializationMs: number;
  let calculate: (request: CalculateBuildRequest) => CalculateBuildResponse;

  beforeAll(async () => {
    const start = performance.now();
    const modulePath = '../wasm/pkg/pobr_wasm.js';
    const wasm = await import(/* @vite-ignore */ modulePath);
    wasm.initSync({ module: await readFile(new URL('../wasm/pkg/pobr_wasm_bg.wasm', import.meta.url)) });
    const root = new URL('../../public/data/', import.meta.url);
    const manifest = JSON.parse(await readFile(new URL('manifest.json', root), 'utf8')) as { version: string; files: string[] };
    version = manifest.version;
    schema = wasm.schemaVersion();
    for (const file of manifest.files) wasm.stageDataFile(file, await readFile(new URL(`${version}/${file}`, root), 'utf8'));
    wasm.initStagedData();
    bridge.optimize = wasm.optimizeVariantsJson;
    calculate = request => JSON.parse(wasm.calculateBuildJson(JSON.stringify(request))) as CalculateBuildResponse;
    // Eligibility metadata is regenerated separately from engine assets. Use its tracked source.
    const source = new URL(`../../../data/${version}/overlay/trade_catalog.json`, import.meta.url);
    catalog = (JSON.parse(await readFile(source, 'utf8')) as TradeCatalog).gems ?? [];
    initializationMs = performance.now() - start;
  }, 120000);

  test('checks exhaustive regret, apply equivalence, weapon bindings and attack/spell isolation', async () => {
    const results: FixtureResult[] = [];
    for (const fixture of fixtures) {
      const active = catalog.find(gem => gem.skill_id === fixture.skill)!;
      expect(active?.compatibility_known).toBe(true);
      const group: SocketGroupInput = { enabled: true, weapon_set: 1,
        gems: [{ skill_id: fixture.skill, level: 16, quality: 0 }] };
      const eligible = eligibleSupports(group, catalog, 85);
      const supportPool = fixture.supports.map(id => eligible.gems.find(gem => gem.skill_id === id)!);
      expect(supportPool.every(Boolean)).toBe(true);
      expect(eligible.gems.map(gem => gem.skill_id)).not.toContain(fixture.skill === 'FireballPlayer'
        ? 'SupportRapidAttacksPlayer' : 'SupportRapidCastingPlayer');
      group.gems.push(gemInput(supportPool[0]));
      const editor: WeaponSetState = {
        items: [{ slot: 'weapon1', text: 'Rarity: RARE\nSynthetic Bow\nTwin Bow\nAdds 30 to 60 Physical Damage\n20% increased Attack Speed' }],
        allocatedNodes: [],
        socketGroups: [group, { enabled: true, weapon_set: 2,
          gems: [{ skill_id: fixture.other, level: 16, quality: 0 }] }],
        weaponSwap: { active: 1, alternate_items: [{ slot: 'weapon1', text: 'Rarity: NORMAL\nAshen Staff' }], exclusive_nodes: [[], []] },
      };
      const request: CalculateBuildRequest = { character: { level: 85, class_name: fixture.className },
        main_socket_group: 0, items: editor.items, socket_groups: groupsForWeaponSet(editor.socketGroups, 1),
        extra_modifiers: ['+1000 to maximum Life', '+1500 to Evasion Rating', '+55% to all Elemental Resistances'],
      };
      const baselineResponse = calculate(request);
      expect(baselineResponse.item_errors).toEqual([]);
      const baseline = statsOf(baselineResponse);
      expect(baseline.TotalDPS).toBeGreaterThan(0);
      expect(baseline.TotalEHP).toBeGreaterThan(0);
      const objective: Objective = { stat: 'TotalDPS', secondaryStat: 'TotalEHP', constraints: [{ stat: 'TotalEHP', min: baseline.TotalEHP }] };
      const smallCatalog = [active, ...supportPool];
      const subsets = Array.from({ length: 1 << supportPool.length }, (_, mask) => supportPool
        .filter((_, index) => Boolean(mask & 1 << index)).map(gemInput))
        .filter(supports => supports.length <= 2 && supportSetCompatible(group, supports, smallCatalog));
      const truthStart = performance.now();
      const truth = await evaluateVariants({ request,
        variants: subsets.map(supports => supportVariant(request, 0, supports, smallCatalog)) });
      const truthMs = performance.now() - truthStart;
      expect(truth.results.every(row => !row.error)).toBe(true);
      const oracle = [...truth.results].sort((a, b) => compareObjectiveStats(a.stats, b.stats, objective) || a.index - b.index)[0];
      const optimum = scoreOf(oracle.stats, objective);
      const summary = (method: string, supports: GemInput[], stats: Record<string, number>, evaluations: number, milliseconds: number): MethodResult => ({
        method, supports: supports.map(gem => gem.skill_id), dps: stats.TotalDPS, ehp: stats.TotalEHP,
        objective: scoreOf(stats, objective), regretPercent: Math.max(0, (optimum - scoreOf(stats, objective)) / optimum * 100),
        evaluations, milliseconds,
      });

      // Independent single-gem ranking is a comparison baseline, not a claim that the old
      // manual subset optimizer could never solve a fully supplied small candidate pool.
      const singleStart = performance.now();
      const singles = await evaluateVariants({ request,
        variants: supportPool.map(gem => supportVariant(request, 0, [gemInput(gem)], smallCatalog)) });
      const greedy: GemInput[] = [];
      for (const row of [...singles.results].sort((a, b) => compareObjectiveStats(a.stats, b.stats, objective))) {
        const next = [...greedy, gemInput(supportPool[row.index])];
        if (supportSetCompatible(group, next, smallCatalog)) greedy.push(gemInput(supportPool[row.index]));
        if (greedy.length === 2) break;
      }
      const greedyStats = statsOf(calculate({ ...request, ...supportVariant(request, 0, greedy, smallCatalog) }));
      const singleMs = performance.now() - singleStart;
      const beamStart = performance.now();
      const beam = await optimizeSupports({ request, groupIndex: 0, catalog: smallCatalog, capacity: 2, objective });
      const beamMs = performance.now() - beamStart;
      expect(beam.plans.length).toBeGreaterThan(0);
      const best = beam.plans[0];
      expect(scoreOf(best.stats, objective)).toBeCloseTo(optimum, 8);

      // Use the editor's actual application helper, then independently run calculateBuild.
      const beforeAlternate = switchWeapons(editor, 2);
      const afterEditor = { ...editor, socketGroups: applySupportPlan(editor.socketGroups, 0, best) };
      expect(afterEditor.socketGroups[1]).toEqual(editor.socketGroups[1]);
      expect(afterEditor.socketGroups[1].enabled).toBe(true);
      const appliedRequest = { ...request, socket_groups: groupsForWeaponSet(afterEditor.socketGroups, 1) };
      expect(appliedRequest.socket_groups).toEqual(best.variant.socket_groups);
      sameStats(statsOf(calculate(appliedRequest)), best.stats);
      const afterAlternate = switchWeapons(afterEditor, 2);
      expect(afterAlternate.items).toEqual(beforeAlternate.items);
      const alternateRequest = (state: WeaponSetState): CalculateBuildRequest => ({ ...request, items: state.items,
        socket_groups: groupsForWeaponSet(state.socketGroups, 2), main_socket_group: 1 });
      sameStats(statsOf(calculate(alternateRequest(afterAlternate))), statsOf(calculate(alternateRequest(beforeAlternate))));

      const spell = fixture.skill === 'FireballPlayer';
      const added = (kind: 'Spells' | 'Attacks') => statsOf(calculate({ ...request,
        extra_modifiers: [...request.extra_modifiers!, `Adds 100 to 200 Physical Damage to ${kind}`] })).TotalDPS;
      const irrelevantGain = added(spell ? 'Attacks' : 'Spells') - baseline.TotalDPS;
      const relevantGain = added(spell ? 'Spells' : 'Attacks') - baseline.TotalDPS;
      expect(irrelevantGain).toBeCloseTo(0, 8);
      expect(relevantGain).toBeGreaterThan(0);
      const result: FixtureResult = { build: fixture.name, candidates: supportPool.map(gem => gem.skill_id),
        allCompatibleCandidates: eligible.gems.length, legalCombinations: subsets.length,
        baseline: { dps: baseline.TotalDPS, ehp: baseline.TotalEHP },
        methods: [summary('independent-single-ranking', greedy, greedyStats, singles.results.length + 1, singleMs),
          summary('bounded-support-combinations', best.supports, best.stats, beam.evaluated, beamMs),
          summary('complete-replacement-oracle', subsets[oracle.index], oracle.stats, truth.results.length, truthMs)],
        irrelevantAddedDamageGain: irrelevantGain, relevantAddedDamageGain: relevantGain,
        applyEquivalent: true, inactiveGroupPreserved: true, alternateWeaponSetPreserved: true,
        unsupported: [...new Set([...(baselineResponse.unsupported_modifiers ?? []), ...truth.results.flatMap(row => row.unsupported ?? [])])],
      };
      results.push(result);
      console.table(result.methods.map(({ supports: _, ...method }) => ({ build: fixture.name, ...method })));
    }
    const cache = new URL('../../../.cache/', import.meta.url);
    await mkdir(cache, { recursive: true });
    await writeFile(new URL('support-algorithm-bench.json', cache), JSON.stringify({ version, schema, initializationMs,
      methodology: 'Real WASM, anonymous synthetic level-85 Ice Shot and Fireball builds; six fixed real eligible supports each, two sockets, all legal subsets enumerated. Balanced geometric DPS/EHP with baseline EHP floor. Independent-single ranking is a comparison baseline; the old manual subset search can also solve a fully supplied small pool. Regret is limited to these fixed pools. Apply uses the editor helper and independent calculateBuild, preserving inactive groups and alternate weapons. Timings are one sequential warm-cache run, not a performance comparison.',
      results }, null, 2), 'utf8');
  }, 30000);
});
