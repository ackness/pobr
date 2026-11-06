import type { BuildSession } from '../hooks/useBuildSession';
import { bindT, type Lang } from '../lib/i18n';
import { baselineTime } from '../lib/comparisonBaseline';
import { Button } from '../components/ui/button';
import type { Route } from './routes';

export function ComparisonBaseline({ session, lang, onNavigate }: { session: BuildSession; lang: Lang; onNavigate: (route: Route) => void }) {
  const tt = bindT(lang);
  const baseline = session.workspace?.builds.find(build => build.id === session.workspace?.activeBuild)?.baseline;
  return <div className="rail-baseline">
    {baseline ? <>
      <span>{tt('compare.baselineRecorded').replace('{time}', baselineTime(baseline.recordedAt, lang))}</span>
      <div><Button size="sm" variant="outline" onClick={() => { session.startBaselineComparison(); onNavigate('compare'); }}>{tt('compare.title')}</Button>
        <Button size="sm" variant="ghost" disabled={session.busy} onClick={session.saveComparisonBaseline}>{tt('compare.recordAgain')}</Button></div>
    </> : <Button size="sm" variant="outline" disabled={session.busy || !session.workspace} onClick={session.saveComparisonBaseline}>{tt('compare.saveBaseline')}</Button>}
  </div>;
}
