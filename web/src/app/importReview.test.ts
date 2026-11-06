import { expect, it } from 'vitest';
import type { BuildSession } from '../hooks/useBuildSession';
import type { CalculateBuildResponse } from '../api/types';
import { advanceImportReview } from './importReview';

const imported = { character: { class_name: 'Witch', ascendancy_name: 'Lich', level: 98 },
  classNames: { classes: {}, ascendancies: {} }, busy: false, calcError: null,
  calc: { stats: [{ id: 'CombinedDPS', value: 1234 }] } as CalculateBuildResponse,
} as BuildSession;

it('waits for the imported calculation, freezes its summary, and replaces it on the next import', () => {
  const pending = advanceImportReview({ scope: null }, 'build:stage', { ...imported, busy: true });
  expect(pending).toEqual({ scope: 'build:stage' });
  const first = advanceImportReview(pending, 'build:stage', imported)!;
  expect(first.summary?.character?.level).toBe(98);
  const edited: BuildSession = { ...imported, character: { ...imported.character!, level: 99 },
    calc: { ...imported.calc!, stats: [{ id: 'CombinedDPS', value: 5678, category: 'Offence' }] } };
  expect(advanceImportReview(first, 'build:stage', edited)).toBe(first);
  const next = advanceImportReview({ scope: null }, 'build:stage', edited)!;
  expect(next.summary?.character?.level).toBe(99);
  expect(next.summary?.calc?.stats[0].value).toBe(5678);
  expect(advanceImportReview(next, 'other:stage', imported)).toBeNull();
  expect(advanceImportReview(null, 'build:stage', imported)).toBeNull();
});

it('retains an imported character with a failed calculation without exposing the retained old result', () => {
  const review = advanceImportReview({ scope: null }, 'build:stage', { ...imported, calcError: 'Calculation failed' });
  expect(review?.summary).toMatchObject({ character: imported.character, calc: null, failed: true });
});
