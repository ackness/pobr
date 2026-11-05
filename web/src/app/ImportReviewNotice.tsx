import { useEffect } from 'react';
import { Button } from '../components/ui/button';
import { bindT, type Lang } from '../lib/i18n';
import type { Route } from './routes';

export function ImportReviewNotice({ route, lang, onDismiss, onReview }: {
  route: Route; lang: Lang; onDismiss: () => void; onReview: () => void;
}) {
  const tt = bindT(lang);
  useEffect(() => { if (route === 'config') onDismiss(); }, [route, onDismiss]);
  if (route === 'config') return null;
  return <div className="shell-notice" role="note"><p>{tt('config.importReview')}</p>
    <Button variant="outline" onClick={onReview}>{tt('config.review')}</Button>
    <Button variant="ghost" onClick={onDismiss}>{tt('config.reviewDismiss')}</Button></div>;
}
