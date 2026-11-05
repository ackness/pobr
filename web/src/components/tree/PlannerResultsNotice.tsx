import { useRef, type ReactNode } from 'react';
import { messages, type Lang } from '../../lib/i18n';
import { AppButton } from '../shared/controls';

/** Keep the completion message beside the action; the result heading owns the destination focus. */
export function PlannerResultsNotice({ count, lang, children }: { count: number; lang: Lang; children: ReactNode }) {
  const heading = useRef<HTMLHeadingElement>(null);
  const text = messages(lang, 'treeOptimizer');
  return <>
    <div className="tree-results-notice" role="status">
      <span>{text.resultsCount.replace('{count}', String(count))}</span>
      <AppButton onClick={() => {
        heading.current?.scrollIntoView({ block: 'start' });
        heading.current?.focus({ preventScroll: true });
      }}>{text.viewResults}</AppButton>
    </div>
    <h4 className="tree-results-heading" tabIndex={-1} ref={heading}>{text.resultsTitle}</h4>
    {children}
  </>;
}
