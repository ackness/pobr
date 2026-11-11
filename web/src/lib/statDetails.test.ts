import { expect, test } from 'vitest';
import { detailFormula, detailInputLabel, detailInputValue, detailEmptyKey, detailKind, StatDetailCache } from './statDetails';
import type { CalculateBuildResponse, Breakdown } from '../api/types';
import details from '../fixtures/stat_details.json' with { type: 'json' };
import declaredInputs from '../fixtures/detail_inputs.json' with { type: 'json' };
import { statNameLabel, type Lang } from './i18n';

test('detail inputs and modifier groups have readable labels in every language', () => {
  const ids = new Set(['SkillGroup2Effect1DPS', ...declaredInputs, ...Object.values(details as Record<string, Breakdown>).flatMap(detail => [
    ...(detail.derived?.inputs ?? []).map(input => input.id),
    ...(detail.derived?.context ?? []).map(input => input.id),
    ...(detail.sources ?? []).map(source => source.name),
  ])]);
  const missing: string[] = [];
  for (const lang of ['en-US', 'zh-TW', 'zh-CN'] as Lang[]) {
    for (const id of ids) {
      // Single-word English names such as Life and Ward are already readable.
      if (lang !== 'en-US' || /[a-z][A-Z]/.test(id)) {
        if (statNameLabel(lang, id) === id) missing.push(`${lang}: ${id}`);
      }
    }
  }
  expect(missing).toEqual([]);
});

test('descriptors enable unopened details and explicitly distinguish none', () => {
  const calc = { breakdowns: {}, breakdown_descriptors: { Str: 'sources', AverageBurstDamage: 'derived', NumberOfDamagingHits: 'none' } } as unknown as CalculateBuildResponse;
  expect(detailKind(calc, 'Str')).toBe('sources');
  expect(detailKind(calc, 'AverageBurstDamage')).toBe('derived');
  expect(detailKind(calc, 'NumberOfDamagingHits')).toBe('none');
});

test('details are deduplicated within a revision but never reused after an edit', async () => {
  const cache = new StatDetailCache();
  let finish!: (value: Breakdown | null) => void;
  let calls = 0;
  const old = cache.load(1, 'Life', () => { calls++; return new Promise(resolve => { finish = resolve; }); });
  expect(cache.load(1, 'Life', async () => { throw Error('duplicate'); })).toBe(old);
  const next = { base_total: 20 } as Breakdown;
  expect(await cache.load(2, 'Life', async () => { calls++; return next; })).toBe(next);
  finish({ base_total: 10 } as Breakdown);
  await old;
  expect(await cache.load(2, 'Life', async () => { throw Error('stale completion'); })).toBe(next);
  expect(calls).toBe(2);
});

test('failed details can be retried and formula ids are localized without calculation', async () => {
  const cache = new StatDetailCache();
  await expect(cache.load(1, 'Life', async () => { throw Error('failed'); })).rejects.toThrow('failed');
  expect(await cache.load(1, 'Life', async () => null)).toBe(null);
  expect(detailFormula('max(0, Life - LifeReserved)', [{ id: 'Life', value: 5 }, { id: 'LifeReserved', value: 2 }], 'zh-CN')).toContain('生命');
});

test.each(['en-US', 'zh-CN', 'zh-TW'] as const)('formulas hide outer rounding and localize operators and inputs in %s', lang => {
  const inputs = [{ id: 'PrimaryMitigatedHit', value: 254122 }, { id: 'PrimaryHitChance', value: 1 }];
  expect(detailFormula('round(PrimaryMitigatedHit * PrimaryHitChance / 2, 9)', inputs, lang))
    .toBe(`${statNameLabel(lang, 'PrimaryMitigatedHit')} × ${statNameLabel(lang, 'PrimaryHitChance')} ÷ 2`);
  expect(detailFormula('round(max(0, PrimaryMitigatedHit) * (1 + PrimaryHitChance), 3)', inputs, lang))
    .toBe(`round(max(0, ${statNameLabel(lang, 'PrimaryMitigatedHit')}) × (1 + ${statNameLabel(lang, 'PrimaryHitChance')}), 3)`);
  expect(detailFormula('round(PrimaryHitChance, 9) + 1', inputs, lang))
    .toBe(`round(${statNameLabel(lang, 'PrimaryHitChance')}, 9) + 1`);
});

test.each([
  ['round(SolvedMaxHit / EnemyDamageMultiplier, 0)', 'round(SolvedMaxHit ÷ EnemyDamageMultiplier, 0)'],
  ['round(round(A,0),9)', 'round(A,0)'],
  ['round(round(A, 9), 9)', 'round(A, 9)'],
  ['round((max(0, A) + (B * (C + D))) / (E + F), 9)', '(max(0, A) + (B × (C + D))) ÷ (E + F)'],
] as const)('formula cleanup preserves meaningful rounding and parentheses: %s', (formula, expected) => {
  expect(detailFormula(formula, [], 'en-US')).toBe(expected);
});

test('primary inputs only gain a main-skill prefix when a secondary pass is present', () => {
  const primary = { id: 'PrimaryHitChance', value: 1 };
  expect(detailInputLabel(primary.id, [primary], 'zh-CN')).toBe('命中率');
  expect(detailInputLabel(primary.id, [primary, { id: 'SecondaryHitChance', value: 0.5 }], 'zh-CN')).toBe('计算技能：命中率');
  expect(detailFormula('ActionRate / 2', [{ id: 'ActionRate', value: 2 }], 'en-US')).toBe('Attack/Cast Rate ÷ 2');
});

test.each([
  ['PrimaryHitChance', 1, '100%'],
  ['SecondaryAccuracyChance', 0.85, '85%'],
  ['PrimaryCriticalChance', 0.2235, '22.4%'],
  ['PrimaryExcessChance', 20, '20%'],
  ['HitChance', 100, '100%'],
  ['CritChance', 22.35, '22.4%'],
  ['FireResist', 20, '20%'],
  ['Life', 1693.83, '1,694'],
  ['ActionRate', 2.3, '2.30'],
  ['PrimaryHitChance', null, '∞'],
] as const)('detail input %s uses its units (%s)', (id, value, expected) => {
  expect(detailInputValue({ id, value }, 'en-US')).toBe(expected);
});

test('empty details distinguish a calculated zero from an unavailable breakdown', () => {
  const empty: Breakdown = { base_total: 0, inc_total: 0, more_total: 1, mods: [], formula: null };
  expect(detailEmptyKey('steps', 0, null)).toBe('calcs.zeroNoRows');
  expect(detailEmptyKey('derived', 0, empty)).toBe('calcs.zeroNoRows');
  expect(detailEmptyKey('none', 0, empty)).toBe('calcs.noBreakdown');
  expect(detailEmptyKey('steps', 123, null)).toBe('calcs.noBreakdown');
  expect(detailEmptyKey('sources', 0, null)).toBe('calcs.noBreakdown');
  expect(detailEmptyKey('steps', null, null)).toBe('calcs.noBreakdown');
  expect(detailEmptyKey('steps', 0, { ...empty, formula: { steps: [{ op: 'base', operand: 0, result: 0 }], value: 0 } })).toBeNull();
  expect(detailEmptyKey('derived', 0, { ...empty, derived: { formula: 'PrimaryHitChance * 0', inputs: [{ id: 'PrimaryHitChance', value: 1 }], context: [] } })).toBeNull();
});
