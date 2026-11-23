import { flushSync } from 'react-dom';
import { NavigateContext, useNavigateToPage } from '../pages/navigate';
import { ChevronRight, Pin, X } from 'lucide-react';
import { BreakdownTable } from '../../components/calcs/CalcsPanel';
import { Button } from '../../components/ui/button';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import { detailKind, detailEmptyKey } from '../../lib/statDetails';
import { statMap, metricMeaning, formatStat } from '../../lib/statDisplay';
import type { SettledCalc } from '../motion/useSettledCalc';
import { parseStatId, statLabel } from './statId';
import { useStatBreakdown } from './useStatBreakdown';

/** "Why is this number what it is": the calculation behind one stat, without leaving the page you are on. */
export default function Inspector({ session, view, lang, id, onClose, onOpenCalcs, pinned, onTogglePin, onNavigate }: {
  session: BuildSession; view: SettledCalc; lang: Lang; id: string; onClose: () => void; onOpenCalcs: (id: string) => void; pinned: boolean; onTogglePin: () => void; onNavigate?: () => void;
}) {
  const navigate = useNavigateToPage();
  const tt = bindT(lang);
  const { stat, minion: minionIndex } = parseStatId(id);
  const minion = minionIndex === undefined ? undefined : view.calc?.minions?.find(row => row.index === minionIndex);
  const source = minion ?? view.calc;
  const raw = minion ? minion.stats[stat] : view.calc ? statMap(view.calc.stats).get(stat) : undefined;
  const { detail, error, loading, retry } = useStatBreakdown(session, id);
  const none = !source || detailKind(source, stat) === 'none';
  const emptyKey = detailEmptyKey(source ? detailKind(source, stat) : 'none', raw, detail);
  return <NavigateContext value={route => {
    flushSync(onNavigate ?? onClose);
    navigate(route);
  }}><div className="insp" data-stale={loading || view.stale || undefined}>
    <header className="insp-head">
      <div>
        <span className="insp-kicker">{minion ? `${tt('minion.title')} · ${minion.name || tt('minion.unavailable')}` : tt('inspector.title')}</span>
        <h2>{stat === 'TotalDPS' ? tt('calcs.hitDps') : statLabel(lang, stat)}</h2>
      </div>
      <div className="insp-actions">
        <Button variant="ghost" size="icon" aria-pressed={pinned} aria-label={pinned ? tt('inspector.unpin') : tt('inspector.pin')} onClick={onTogglePin}><Pin /></Button>
        <Button variant="ghost" size="icon" aria-label={tt('common.close')} onClick={onClose}><X /></Button>
      </div>
    </header>
    <p className="insp-value">{raw === undefined ? '—' : formatStat(stat, raw, lang)}</p>
    {metricMeaning(stat, lang) && <p className="metric-meaning">{metricMeaning(stat, lang)}</p>}
    <div className="insp-body" key={id}>
      {none ? <p className="insp-note" role="status">{tt('calcs.noBreakdown')}</p>
        : error ? <div className="insp-note" role="alert">{error} <Button variant="outline" size="sm" onClick={retry}>{tt('calcs.retry')}</Button></div>
          : detail ? emptyKey ? <p className="insp-note" role="status">{tt(emptyKey)}</p> : <BreakdownTable breakdown={detail} lang={lang} stat={stat} values={minion ? new Map(Object.entries(minion.stats)) : statMap(view.calc?.stats ?? [])} related={source?.breakdowns} onSelect={target => onOpenCalcs(minionIndex === undefined ? target : `Minion:${minionIndex}:${target}`)} />
            : <p className="insp-note" role="status">{tt(loading ? 'calcs.loadingDetail' : emptyKey!)}</p>}
    </div>
    <footer className="insp-foot"><Button variant="ghost" size="sm" onClick={() => onOpenCalcs(id)}>{tt('inspector.openInCalcs')}<ChevronRight /></Button></footer>
  </div></NavigateContext>;
}
