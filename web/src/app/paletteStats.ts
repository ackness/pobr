import type { CalculateBuildResponse } from '../api/types';
import type { Lang } from '../lib/i18n';
import { STAT_SECTIONS, formatStat } from '../lib/statDisplay';
import { statLabel } from './inspector/statId';
import { HEADLINE_STATS } from './preview/changeSummary';

const COMMON = [...HEADLINE_STATS, 'TotalDPS', 'CritChance', 'CritMultiplier', 'HitChance', 'ActionRate',
  'FireResistMax', 'ColdResistMax', 'LightningResistMax', 'ChaosResistMax'];
const CURATED = new Set([...COMMON, ...STAT_SECTIONS.flatMap(section => section.rows.map(row => row.id))]);

export function paletteStats(calc: CalculateBuildResponse | null, query: string, lang: Lang, limit = 30) {
  const q = query.trim().toLowerCase();
  if (!calc || !q) return [];
  const priority = (id: string) => { const index = COMMON.indexOf(id); return index >= 0 ? index : CURATED.has(id) ? COMMON.length : COMMON.length + 1; };
  return calc.stats.filter(stat => {
    if (!CURATED.has(stat.id) && (stat.value === 0 || stat.value === null)) return false;
    return [stat.id, statLabel(lang, stat.id)].some(text => text.toLowerCase().includes(q));
  }).sort((a, b) => priority(a.id) - priority(b.id) || a.id.localeCompare(b.id))
    .slice(0, limit).map(stat => ({ id: stat.id, label: statLabel(lang, stat.id), value: formatStat(stat.id, stat.value, lang) }));
}
