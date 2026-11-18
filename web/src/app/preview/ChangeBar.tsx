import { usePreview } from '../../components/shared/PreviewContext';
import { formatStat } from '../../lib/statDisplay';
import { bindT, type Lang } from '../../lib/i18n';
import { statLabel } from '../inspector/statId';
import { percentText } from '../motion/statChanges';

/** What the candidate under the pointer would do, as a bar along the bottom. Shown only while something is previewed. */
export function ChangeBar({ lang }: { lang: Lang }) {
  const preview = usePreview();
  const state = preview?.state;
  if (!state) return null;
  const tt = bindT(lang);
  return <div className="shell-changebar" aria-hidden={state.status === 'loading' || undefined}>
    <div className="cb" role="status" aria-live="polite" data-status={state.status}>
      <span className="cb-label">{state.label}</span>
      {state.status === 'loading' ? <span className="cb-note">{tt('build.calculating')}</span>
        : state.changes.length === 0 ? <span className="cb-note">{tt('preview.noStatChanges')}</span>
          : <ul>{state.changes.map(change => <li key={change.id} data-tone={change.tone}>
            <span className="cb-stat">{statLabel(lang, change.id)}</span>
            <span className="cb-values">{formatStat(change.id, change.before, lang)} → {formatStat(change.id, change.after, lang)}</span>
            <strong>{percentText(change)}</strong></li>)}</ul>}
    </div>
  </div>;
}
