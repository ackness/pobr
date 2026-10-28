import { ArrowUpRight, Pin, X } from 'lucide-react';
import { BreakdownTable } from '../../components/calcs/CalcsPanel';
import { Button } from '../../components/ui/button';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import { detailKind } from '../../lib/statDetails';
import { formatStatValue, statMap } from '../../lib/statDisplay';
import type { SettledCalc } from '../motion/useSettledCalc';
import { words } from '../text';
import { parseStatId, statFormat, statLabel } from './statId';
import { useStatBreakdown } from './useStatBreakdown';

/** "Why is this number what it is": the calculation behind one stat, without leaving the page you are on. */
export default function Inspector({ session, view, lang, id, onClose, onOpenCalcs, pinned, onTogglePin }: {
  session: BuildSession; view: SettledCalc; lang: Lang; id: string; onClose: () => void; onOpenCalcs: (id: string) => void; pinned: boolean; onTogglePin: () => void;
}) {
  const tt = bindT(lang);
  const w = (en: string, cn: string, tw?: string) => words(lang, en, cn, tw);
  const { stat, minion: minionIndex } = parseStatId(id);
  const minion = minionIndex === undefined ? undefined : view.calc?.minions?.find(row => row.index === minionIndex);
  const source = minion ?? view.calc;
  const raw = minion ? minion.stats[stat] : view.calc ? statMap(view.calc.stats).get(stat) : undefined;
  const { detail, error, loading, retry } = useStatBreakdown(session, id);
  const none = !source || detailKind(source, stat) === 'none';
  return <div className="insp" data-stale={loading || view.stale || undefined}>
    <header className="insp-head">
      <div>
        <span className="insp-kicker">{minion ? `${tt('minion.title')} · ${minion.name || tt('minion.unavailable')}` : w('Why this number', '这个数从哪来', '這個數從哪來')}</span>
        <h2>{statLabel(lang, stat)}</h2>
      </div>
      <div className="insp-actions">
        <Button variant="ghost" size="icon" aria-pressed={pinned} aria-label={pinned ? w('Unpin from the results panel', '从结果栏取消固定', '從結果欄取消固定') : w('Pin to the results panel', '固定到结果栏', '固定到結果欄')} onClick={onTogglePin}><Pin /></Button>
        <Button variant="ghost" size="icon" aria-label={w('Close', '关闭', '關閉')} onClick={onClose}><X /></Button>
      </div>
    </header>
    <p className="insp-value">{raw === undefined ? '—' : formatStatValue(raw, minion ? 'float2' : statFormat(stat), lang)}</p>
    <div className="insp-body" key={id}>
      {none ? <p className="insp-note" role="status">{tt('calcs.noBreakdown')}</p>
        : error ? <div className="insp-note" role="alert">{error} <Button variant="outline" size="sm" onClick={retry}>{tt('calcs.retry')}</Button></div>
          : detail ? <BreakdownTable breakdown={detail} lang={lang} />
            : <p className="insp-note" role="status">{loading ? tt('calcs.loadingDetail') : tt('calcs.noBreakdown')}</p>}
    </div>
    <footer className="insp-foot"><Button variant="ghost" size="sm" onClick={() => onOpenCalcs(id)}>{w('Open in Calculations', '在计算页中打开', '在計算頁中開啟')}<ArrowUpRight /></Button></footer>
  </div>;
}
