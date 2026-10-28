import { usePreview } from '../../components/shared/PreviewContext';
import { formatStatValue } from '../../lib/statDisplay';
import type { Lang } from '../../lib/i18n';
import { statFormat, statLabel } from '../inspector/statId';
import type { StatChange } from '../motion/statChanges';
import { words } from '../text';

function percentText(change: StatChange) {
  if (change.percent === null) return `${change.delta > 0 ? '+' : '−'}${formatStatValue(Math.abs(change.delta), 'float2')}`;
  return `${change.percent > 0 ? '+' : '−'}${Math.abs(change.percent).toFixed(1)}%`;
}

/** What the candidate under the pointer would do, as a bar along the bottom. Shown only while something is previewed. */
export function ChangeBar({ lang }: { lang: Lang }) {
  const preview = usePreview();
  const state = preview?.state;
  if (!state) return null;
  const w = (en: string, cn: string, tw?: string) => words(lang, en, cn, tw);
  return <div className="v2-changebar" aria-hidden={state.status === 'loading' || undefined}>
    <div className="cb" role="status" aria-live="polite" data-status={state.status}>
      <span className="cb-label">{state.label}</span>
      {state.status === 'loading' ? <span className="cb-note">{w('Calculating…', '计算中…', '計算中…')}</span>
        : state.changes.length === 0 ? <span className="cb-note">{w('No stat changes', '没有任何数值变化', '沒有任何數值變化')}</span>
          : <ul>{state.changes.map(change => <li key={change.id} data-tone={change.tone}>
            <span className="cb-stat">{statLabel(lang, change.id)}</span>
            <span className="cb-values">{formatStatValue(change.before, statFormat(change.id), lang)} → {formatStatValue(change.after, statFormat(change.id), lang)}</span>
            <strong>{percentText(change)}</strong></li>)}</ul>}
    </div>
  </div>;
}
