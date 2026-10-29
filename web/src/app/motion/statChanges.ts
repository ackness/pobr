import type { DisplayStatValue } from '../../api/types';
import { compareStats } from '../../lib/compare';
import { statPolarity } from './statPolarity';

export type Tone = 'better' | 'worse' | 'neutral';
export interface StatChange { id: string; before: number; after: number; delta: number; percent: number | null; tone: Tone }

/** What an edit moved, stat by stat. Unbounded or missing values are never turned into numbers. */
export function computeStatChanges(previous: DisplayStatValue[] | null, current: DisplayStatValue[]): Map<string, StatChange> {
  const changes = new Map<string, StatChange>();
  if (!previous) return changes;
  for (const row of compareStats(previous, current)) {
    if (!row.changed || row.delta === null || typeof row.before !== 'number' || typeof row.after !== 'number') continue;
    const polarity = statPolarity(row.id);
    changes.set(row.id, { id: row.id, before: row.before, after: row.after, delta: row.delta, percent: row.percent,
      tone: polarity === 0 ? 'neutral' : row.delta * polarity > 0 ? 'better' : 'worse' });
  }
  return changes;
}
