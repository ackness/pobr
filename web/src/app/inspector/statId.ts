import { STAT_SECTIONS, type StatFormat } from '../../lib/statDisplay';
import { statNameLabel, type Lang } from '../../lib/i18n';

/** A stat id as the rail and the calculation page pass it around: `Life` or `Minion:<index>:<stat>`. */
export function parseStatId(id: string): { stat: string; minion?: number } {
  const match = /^Minion:(\d+):(.+)$/.exec(id);
  return match ? { stat: match[2], minion: Number(match[1]) } : { stat: id };
}

const ROWS = new Map(STAT_SECTIONS.flatMap(section => section.rows.map(row => [row.id, row] as const)));

export function statFormat(stat: string): StatFormat { return ROWS.get(stat)?.format ?? 'float2'; }
export function statLabel(lang: Lang, stat: string): string { return ROWS.get(stat)?.label[lang] ?? statNameLabel(lang, stat); }
export function statColor(stat: string): string | undefined { const colorVar = ROWS.get(stat)?.colorVar; return colorVar ? `var(--${colorVar})` : undefined; }
