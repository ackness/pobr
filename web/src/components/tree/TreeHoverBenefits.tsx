import type { DiffEntry } from '../../lib/compare';
import { formatComparisonDelta } from '../../lib/compare';
import { bindT, type Lang } from '../../lib/i18n';
import { treeStatLabel } from './treePresentation';
import { STAT_SECTIONS, statFormat } from '../../lib/statDisplay';

const NAMED_STATS = new Set(STAT_SECTIONS.flatMap(section => section.rows.map(row => row.id)));

/** Prefer whole-build outcomes; rank remaining changes by relative size, not raw units. */
export function treeHoverChanges(diffs: DiffEntry[], lang: Lang = 'en-US') {
  const priority = ['CombinedDPS', 'TotalEHP', 'Life'];
  const changed = diffs.filter(diff => Number.isFinite(diff.delta) && Math.abs(diff.delta) >= 0.005);
  const primary = priority.flatMap(id => changed.filter(diff => diff.id === id));
  const rest = changed.filter(diff => !priority.includes(diff.id)).sort((a, b) =>
    Math.abs(b.delta) / Math.max(1, Math.abs(b.before)) - Math.abs(a.delta) / Math.max(1, Math.abs(a.before)) || a.id.localeCompare(b.id));
  const redundantDamage = (diff: DiffEntry) => primary.some(row => row.id === 'CombinedDPS')
    && (diff.id === 'TotalDPS' || /^With.*DPS$/.test(diff.id) || ['CullingDPS', 'CombinedAvg', 'TotalHitAvg', 'AverageDamage'].includes(diff.id));
  const important = rest.filter(diff => !redundantDamage(diff) && (NAMED_STATS.has(diff.id) || treeStatLabel(lang, diff.id) !== diff.id)).slice(0, 2);
  const selected = new Set(important.map(diff => diff.id));
  return { primary: [...primary, ...important], more: rest.filter(diff => !selected.has(diff.id)) };
}

export function TreeHoverBenefits({ diffs, lang, refundCount = 0 }: { diffs: DiffEntry[]; lang: Lang; refundCount?: number }) {
  const tt = bindT(lang);
  const { primary, more } = treeHoverChanges(diffs, lang);
  const row = (diff: DiffEntry) => <span key={diff.id} className={`diff-entry ${diff.delta > 0 ? 'diff-pos' : 'diff-neg'}`}>
    {treeStatLabel(lang, diff.id)} {formatComparisonDelta(diff.delta, statFormat(diff.id), lang)}
  </span>;
  return <div className="diff-list">
    {refundCount > 1 && <p>{tt('tree.downstream').replace('{count}', String(refundCount - 1))}</p>}
    {primary.map(row)}
    {primary.length === 0 && <span>{tt('diff.none')}</span>}
    {more.length > 0 && <details><summary className="disclosure" data-disclosure-level="inline">{tt('tree.moreChanges').replace('{count}', String(more.length))}</summary>{more.map(row)}</details>}
  </div>;
}
