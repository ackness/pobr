import { expect, test } from 'vitest';
import { detailFormula, detailKind, StatDetailCache } from './statDetails';
import type { CalculateBuildResponse, Breakdown } from '../api/types';
import details from '../fixtures/stat_details.json';
import { statNameLabel, type Lang } from './i18n';

test('detail inputs and modifier groups have readable labels in every language', () => {
  const ids = new Set(Object.values(details as Record<string, Breakdown>).flatMap(detail => [
    ...(detail.derived?.inputs ?? []).map(input => input.id),
    ...(detail.derived?.context ?? []).map(input => input.id),
    ...(detail.sources ?? []).map(source => source.name),
  ]));
  for (const lang of ['en-US', 'zh-TW', 'zh-CN'] as Lang[]) {
    for (const id of ids) {
      // Single-word English names such as Life and Ward are already readable.
      if (lang !== 'en-US' || /[a-z][A-Z]/.test(id)) {
        expect(statNameLabel(lang, id), `${lang}: ${id}`).not.toBe(id);
      }
    }
  }
});

test('descriptors enable unopened details and explicitly distinguish none', () => {
  const calc = { breakdowns: {}, breakdown_descriptors: { ManaCost: 'sources', TotalEHP: 'derived', Str: 'none' } } as unknown as CalculateBuildResponse;
  expect(detailKind(calc, 'ManaCost')).toBe('sources');
  expect(detailKind(calc, 'TotalEHP')).toBe('derived');
  expect(detailKind(calc, 'Str')).toBe('none');
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
