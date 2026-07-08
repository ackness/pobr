import type { DiffEntry } from '../../lib/compare';
import { bindT, statNameLabel, type Lang } from '../../lib/i18n';
import { formatStatValue } from '../../lib/statDisplay';
import './diff.css';

/** 对比差异列表（装备切换预览 / 天赋点收益共用）。 */
export function DiffList({
  diffs,
  lang,
  limit = 8,
}: {
  diffs: DiffEntry[];
  lang: Lang;
  limit?: number;
}) {
  const tt = bindT(lang);
  if (diffs.length === 0) {
    return <span className="diff-none">{tt('diff.none')}</span>;
  }
  return (
    <span className="diff-list">
      {diffs.slice(0, limit).map((d) => (
        <span key={d.id} className={`diff-entry ${d.delta > 0 ? 'diff-pos' : 'diff-neg'}`}>
          {statNameLabel(lang, d.id)} {d.delta > 0 ? '+' : ''}
          {formatStatValue(d.delta, 'float2')}
        </span>
      ))}
      {diffs.length > limit && <span className="diff-more">+{diffs.length - limit}</span>}
    </span>
  );
}
