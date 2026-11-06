import { colon } from '../../app/text';
import type { TreeEditResponse } from '../../api/types';
import { bindT, type Lang } from '../../lib/i18n';
import type { passivePointSummary } from './treePresentation';

export function TreePointSummary({ current, preview, invalidReason, lang }: {
  current: ReturnType<typeof passivePointSummary>; preview: TreeEditResponse | null; invalidReason: string; lang: Lang;
}) {
  const text = bindT(lang);
  const result = preview?.result;
  const used = result?.points_used ?? current.used;
  const max = result?.points_max ?? current.max;
  const change = result && current.used !== null ? result.points_used - current.used : null;
  const separator = colon(lang);
  return <>
    <span className="tree-count" role="status">
      {text('tree.usedPoints').replace('{used}', used === null ? '—' : String(used)).replace('{max}', String(max))}
      <span className="tree-count-change">{change !== null ? text('tree.pointChange').replace('{delta}', `${change >= 0 ? '+' : ''}${change}`) : ''}</span>
    </span>
    <p className="tree-edit-preview" role="status" aria-live="polite" aria-atomic="true">{preview && !result ? invalidReason : ''}</p>
    <details className="tree-point-details"><summary className="disclosure">{text('tree.pointDetails')}</summary>
      <p>{text('tree.pointBudgetHint')}</p>
      <p>{text('tree.campaignPoints')}</p>
      <p>{text('tree.set1')}{separator}{result?.weapon_points_used[0] ?? current.weapon[0]}</p>
      <p>{text('tree.set2')}{separator}{result?.weapon_points_used[1] ?? current.weapon[1]}</p>
      {result && <p>{text('tree.previewChanges')}{separator}+{result.added.length} / −{result.removed.length} · {text('tree.promoted')}{separator}{result.promoted.length}</p>}
    </details>
  </>;
}
