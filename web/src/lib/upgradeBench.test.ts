/** Opt-in real-engine experiment. Run with POBR_UPGRADE_BENCH=1; results are not game-wide guarantees. */
import { beforeAll, describe, expect, test, vi } from 'vitest';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { env } from 'node:process';
import type { CalculateBuildRequest, OptimizeVariantsResponse, VariantInput } from '../api/types';
import { compareObjectiveStats, evaluateVariants, scoreOf, type Objective } from './optimize';
import { affixPool, combinationLegal, combinationText, optimizeTradeAffixes, tradeItemVariant, type TradeCatalog } from './tradeOptimizer';
import { scoreEquipment } from './equipmentScore';
import { type WeightedStat } from './trade';

const bridge = vi.hoisted(() => ({ optimize: undefined as undefined | ((input: string) => string) }));
vi.mock('../api/backend', () => ({ getBackend: async () => ({ optimizeVariants: async (request: unknown) =>
  JSON.parse(bridge.optimize!(JSON.stringify(request))) as OptimizeVariantsResponse }) }));

interface ExperimentResult {
  build: string;
  candidates: number;
  baseline: Record<string, number>;
  methods: { method: string; dps: number; ehp: number; objective: number; regretPercent: number; dpsDowngrade: boolean;
    evaluations: number; milliseconds: number; selectedMods: string[]; misorderedPairs?: number; comparablePairs?: number;
    higherSumWorseObjective?: number; higherSumLowerDps?: number }[];
  irrelevantAttackWeight?: number;
  unsupported: string[];
}

const fixtures = [
  { name: 'attack-ice-shot-bow', skill: 'IceShotPlayer', className: 'Ranger', base: 'Twin Bow', slot: 'weapon1',
    groups: ['LocalPhysicalDamage', 'LocalPhysicalDamagePercent', 'LocalColdDamage', 'LocalAccuracyRating',
      'LocalIncreasedAttackSpeed', 'LocalBaseCriticalStrikeChance', 'LocalCriticalStrikeMultiplier', 'GlobalIncreaseProjectileSkillGemLevelWeapon'] },
  { name: 'spell-fireball-ring', skill: 'FireballPlayer', className: 'Sorceress', base: 'Sapphire Ring', slot: 'ring1',
    groups: ['IncreasedLife', 'SpellDamage', 'FireDamagePercentage', 'FireDamage',
      'IncreasedCastSpeed', 'FireResistance', 'LightningResistance', 'SpellCriticalStrikeChance'] },
] as const;

describe.skipIf(env.POBR_UPGRADE_BENCH !== '1')('real WASM upgrade algorithm comparison', () => {
  let catalog: TradeCatalog;
  let version: string;
  let schema: number;
  let initializationMs: number;

  beforeAll(async () => {
    const start = performance.now();
    // Generated wasm-pack output is optional for ordinary unit-test runs.
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
    catalog = JSON.parse(await readFile(new URL(`${version}/overlay/trade_catalog.json`, root), 'utf8')) as TradeCatalog;
    initializationMs = performance.now() - start;
  }, 120000);

  test('compares old/new weighted Sum against every legal candidate replacement', async () => {
    const report: ExperimentResult[] = [];
    for (const fixture of fixtures) {
      const base = catalog.bases.find(entry => entry.name === fixture.base)!;
      expect(base).toBeDefined();
      const available = affixPool(catalog, base, 82);
      const pool = fixture.groups.map(group => available.find(mod => mod.group === group)!);
      expect(pool.every(Boolean)).toBe(true);
      // Pick the same fixed indices before observing engine outputs; no winning case selection.
      const currentMods = [0, 1, 3, 4, 5, 6].map(index => pool[index]);
      expect(combinationLegal(currentMods)).toBe(true);
      const current = combinationText(base, currentMods, 82);
      const request: CalculateBuildRequest = { character: { level: 85, class_name: fixture.className },
        items: [{ slot: fixture.slot, text: current }],
        socket_groups: [{ enabled: true, gems: [{ skill_id: fixture.skill, level: 16, quality: 0 }] }], main_socket_group: 0,
        extra_modifiers: ['+1000 to maximum Life', '+1500 to Evasion Rating', '+55% to all Elemental Resistances'],
      };
      const candidates = Array.from({ length: 1 << pool.length }, (_, mask) => pool.filter((_, index) => mask & 1 << index))
        .filter(mods => combinationLegal(mods)).map(mods => ({ mods, text: combinationText(base, mods, 82) }));
      const variant = (text: string): VariantInput => tradeItemVariant(request, fixture.slot, text);
      const objective: Objective = { stat: 'TotalDPS', secondaryStat: 'TotalEHP', constraints: [] };
      const probes = [...new Map(pool.flatMap(mod => mod.stats).map(stat => [stat.id, stat])).values()];
      const blank = combinationText(base, [], 82);

      // v0.0.11 formula: average additions to the blank item and the equipped item,
      // scale by 1000 / current score, keep positive weights, round at query time.
      // Both algorithms use the same small pool rather than the full category catalog.
      const oldStart = performance.now();
      const oldProbes = await evaluateVariants({ request,
        variants: [blank, current].flatMap(text => [variant(text), ...probes.map(stat => variant(`${text}\n${stat.line}`))]) });
      expect(oldProbes.results.every(row => !row.error)).toBe(true);
      const baseline = oldProbes.baseline;
      expect(baseline.TotalDPS).toBeGreaterThan(0);
      expect(baseline.TotalEHP).toBeGreaterThan(0);
      const scale = 1000 / Math.max(Math.abs(scoreOf(baseline, objective)), Math.abs(scoreOf(oldProbes.results[0].stats, objective)), 1);
      const old: WeightedStat[] = probes.flatMap((stat, index) => {
        const gain = [0, 1].reduce((sum, context) => {
          const offset = context * (probes.length + 1);
          return sum + scoreOf(oldProbes.results[offset + index + 1].stats, objective) - scoreOf(oldProbes.results[offset].stats, objective);
        }, 0) / 2;
        return gain > 0 ? [{ ...stat, weight: gain / stat.value * scale }] : [];
      }).sort((a, b) => b.weight * b.value - a.weight * a.value).slice(0, 32);
      const oldMs = performance.now() - oldStart;
      const newStart = performance.now();
      const optimized = await optimizeTradeAffixes({ request, slot: fixture.slot, base, pool, itemLevel: 82, objective, combinations: false });
      const newMs = performance.now() - newStart;
      const truthStart = performance.now();
      const truth = await evaluateVariants({ request, variants: candidates.map(candidate => variant(candidate.text)) });
      const truthMs = performance.now() - truthStart;
      expect(truth.results.every(row => !row.error)).toBe(true);
      const oracle = [...truth.results].sort((a, b) => compareObjectiveStats(a.stats, b.stats, objective) || a.index - b.index)[0];
      const bestScore = scoreOf(oracle.stats, objective);
      const choose = (weights: WeightedStat[]) => candidates.map((candidate, index) => ({ index, score: scoreEquipment(candidate.text, weights, probes).score }))
        .sort((a, b) => b.score - a.score || a.index - b.index)[0].index;
      const rankingErrors = (weights: WeightedStat[]) => {
        const predictions = candidates.map(candidate => scoreEquipment(candidate.text, weights, probes).score);
        const scores = truth.results.map(row => scoreOf(row.stats, objective));
        let misorderedPairs = 0, comparablePairs = 0;
        for (let i = 0; i < candidates.length; i += 1) for (let j = i + 1; j < candidates.length; j += 1) {
          if (Math.abs(scores[i] - scores[j]) < 1e-8 || Math.abs(predictions[i] - predictions[j]) < 1e-8) continue;
          comparablePairs += 1;
          if ((scores[i] - scores[j]) * (predictions[i] - predictions[j]) < 0) misorderedPairs += 1;
        }
        const currentSum = scoreEquipment(current, weights, probes).score;
        return { misorderedPairs, comparablePairs,
          higherSumWorseObjective: predictions.filter((sum, index) => sum > currentSum + 1e-8 && scores[index] < scoreOf(baseline, objective) - 1e-8).length,
          higherSumLowerDps: predictions.filter((sum, index) => sum > currentSum + 1e-8 && truth.results[index].stats.TotalDPS < baseline.TotalDPS - 1e-8).length };
      };
      const summary = (method: string, index: number, evaluations: number, milliseconds: number, weights?: WeightedStat[]) => {
        const stats = truth.results[index].stats;
        return { method, dps: stats.TotalDPS, ehp: stats.TotalEHP, objective: scoreOf(stats, objective),
          regretPercent: Math.max(0, (bestScore - scoreOf(stats, objective)) / bestScore * 100),
          dpsDowngrade: stats.TotalDPS < baseline.TotalDPS, evaluations, milliseconds,
          selectedMods: candidates[index].mods.map(mod => mod.id), ...(weights ? rankingErrors(weights) : {}) };
      };
      const entry: ExperimentResult = { build: fixture.name, candidates: candidates.length, baseline,
        methods: [summary('v0.0.11-additions', choose(old), oldProbes.results.length, oldMs, old),
          summary('removal-marginals', choose(optimized.weighted), optimized.evaluated, newMs, optimized.weighted),
          summary('complete-replacement-oracle', oracle.index, truth.results.length, truthMs)],
        unsupported: [...new Set([...oldProbes.results, ...truth.results].flatMap(row => row.unsupported ?? []))] };
      if (fixture.skill === 'FireballPlayer') {
        const attackIds = new Set(pool.filter(mod => mod.group === 'FireDamage').flatMap(mod => mod.stats.map(stat => stat.id)));
        entry.irrelevantAttackWeight = optimized.weighted.filter(stat => attackIds.has(stat.id)).reduce((sum, stat) => sum + Math.abs(stat.weight), 0);
        expect(entry.irrelevantAttackWeight).toBe(0);
      }
      report.push(entry);
      console.table(entry.methods.map(({ selectedMods: _, ...method }) => ({ build: entry.build, ...method })));
    }
    const cache = new URL('../../../.cache/', import.meta.url);
    await mkdir(cache, { recursive: true });
    await writeFile(new URL('upgrade-algorithm-bench.json', cache), JSON.stringify({ version, schema, initializationMs,
      methodology: 'Real WASM; synthetic level-85 builds; eight fixed real catalog affixes each; every legal subset at maximum rolls on one fixed base; balanced geometric DPS/EHP; no price, rotation or global market optimality claim; old formula reproduced on the same reduced pool. Timing is one observed sequential run, affected by warm caches/JIT, not a performance comparison. Tied pairs are excluded from inversion counts.',
      results: report }, null, 2), 'utf8');
    expect(report).toHaveLength(2);
  }, 120000);
});
