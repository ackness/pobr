import { colon } from '../../app/text';
import type { TreeEditResponse } from '../../api/types';
import { bindT, type Lang } from '../../lib/i18n';
import type { passivePointSummary } from './treePresentation';

export function TreePointSummary({ current, preview, ascendancy, invalidReason, lang }: {
  current: ReturnType<typeof passivePointSummary>; preview: TreeEditResponse | null;
  ascendancy: number | null; invalidReason: string; lang: Lang;
}) {
  const text = bindT(lang);
  const result = preview?.result;
  const used = result?.points_used ?? current.used;
  const max = result?.points_max ?? current.max;
  const change = result && current.used !== null ? result.points_used - current.used : null;
  const excess = used !== null && max !== null ? Math.max(0, used - max) : 0;
  const separator = colon(lang);
  return <>
    <span className="tree-count" role="status">
      <span className="tree-count-kind">{text('tree.ordinaryPoints')} · </span>
      {text('tree.usedPoints').replace('{used}', used === null ? '—' : String(used)).replace('{max}', max === null ? '—' : String(max))}
      {excess > 0 && <strong className="tree-count-excess">{text('tree.pointsOverBudget').replace('{count}', String(excess))}</strong>}
      <span className="tree-count-change">{change !== null ? text('tree.pointChange').replace('{delta}', `${change >= 0 ? '+' : ''}${change}`) : ''}</span>
    </span>
    <p className="tree-edit-preview" role="status" aria-live="polite" aria-atomic="true">{preview && !result ? invalidReason : ''}</p>
    <details className="tree-point-details"><summary className="disclosure">{text('tree.pointDetails')}</summary>
      <div className="tree-point-breakdown">
        <p>{text('tree.pointBudgetHint')}</p>
        <p>{text('tree.campaignPoints')}</p>
        <dl>
          <div><dt>{text('tree.set1')}</dt><dd>{result?.weapon_points_used[0] ?? current.weapon[0]}</dd></div>
          <div><dt>{text('tree.set2')}</dt><dd>{result?.weapon_points_used[1] ?? current.weapon[1]}</dd></div>
          <div><dt>{text('tree.ascendancy')}</dt><dd>{ascendancy ?? '—'}</dd></div>
        </dl>
        {excess > 0 && <p className="tree-point-source-note">{text('tree.pointSourceUncertain')}</p>}
        {max === null && <p>{text('tree.pointBudgetUnavailable')}</p>}
        {result && <p>{text('tree.previewChanges')}{separator}+{result.added.length} / −{result.removed.length} · {text('tree.promoted')}{separator}{result.promoted.length}</p>}
      </div>
    </details>
  </>;
}
