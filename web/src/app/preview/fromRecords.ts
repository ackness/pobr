import type { DisplayStatValue } from '../../api/types';
import { computeStatChanges, type StatChange } from '../motion/statChanges';
import { summarizeChanges } from './changeSummary';

const asStats = (record: Record<string, number>): DisplayStatValue[] => Object.entries(record).map(([id, value]) => ({ id, value, category: 'Offence' }));

/**
 * What a ranked result changes, read from the stats the engine already returned for it (a record of
 * the stats that were collected, not the whole catalogue). Nothing is calculated again.
 */
export function changesFromRecords(baseline: Record<string, number>, stats: Record<string, number>): StatChange[] {
  return summarizeChanges(computeStatChanges(asStats(baseline), asStats(stats)));
}
