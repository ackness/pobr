import { useEffect } from 'react';
import { X } from 'lucide-react';
import { Button } from '../components/ui/button';
import { bindT, type Lang } from '../lib/i18n';
import type { Route } from './routes';
import type { ImportSummary } from './importReview';

export function ImportReviewNotice({ route, lang, summary, empty = false, onDismiss, onReview, onReviewNotes, modifierCount }: {
  route: Route; lang: Lang; summary: ImportSummary; empty?: boolean; onDismiss: () => void; onReview: () => void;
  onReviewNotes?: () => void; modifierCount?: number;
}) {
  const tt = bindT(lang);
  useEffect(() => { if (route === 'config') onDismiss(); }, [route, onDismiss]);
  if (route === 'config') return null;
  const count = modifierCount ?? summary.calc?.unsupported_modifiers.length ?? 0;
  const overviewHasModifiers = route === 'overview' && !!summary.calc && count > 0;
  return <div className="shell-notice import-summary" role="note" aria-label={tt('build.importSummary')}>
    <div className="import-summary-body">
      {empty ? <p><strong>{tt('build.importedEmptyStage')}</strong></p> : <p><strong>{tt('build.imported')}</strong>{' · '}{tt('config.importReview')}{' '}
        <a href="/config" onClick={event => { event.preventDefault(); onReview(); }}>{tt('config.review')}</a>
        {!summary.calc && <>{' · '}{tt(summary.failed ? 'build.summaryFailed' : 'build.summaryPending')}</>}
        {overviewHasModifiers && <>{' · '}<a href="#ov-unincluded" onClick={event => {
          const target = document.getElementById('ov-unincluded');
          if (!target) return;
          event.preventDefault();
          const details = target.closest('details');
          if (details) details.open = true;
          target.focus();
        }}>{tt('build.summaryModifiersLink').replace('{count}', String(count))}</a></>}
      </p>}
      {!empty && summary.importSource === 'wegame' && <p>{tt('build.wegameReview')}{' '}
        <a href="/build#bd-notes" onClick={onReviewNotes ? event => { event.preventDefault(); onReviewNotes(); } : undefined}>{tt('build.reviewImportNotes')}</a>{' '}
        {tt('build.wegameWeaponSets')}
      </p>}
    </div>
    <Button variant="ghost" size="icon" aria-label={tt('config.reviewDismiss')} title={tt('config.reviewDismiss')} onClick={onDismiss}><X /></Button>
  </div>;
}
