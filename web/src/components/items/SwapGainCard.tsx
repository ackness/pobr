import type { DisplayStatValue } from '../../api/types';
import { compareStats, formatComparisonDelta } from '../../lib/compare';
import { bindT, statNameLabel, type Lang } from '../../lib/i18n';
import { formatNumber, formatStatValue, statFormat } from '../../lib/statDisplay';
import { upgradeT } from '../../lib/upgradeText';
import type { DamageHeadline } from '../../lib/mainMinion';
import './swapGainCard.css';


/** The same before/after projection for crafted items and saved-item comparisons. */
export function SwapGainCard({ before, after, lang, damage }: { before: readonly Pick<DisplayStatValue, 'id' | 'value'>[]; after: readonly Pick<DisplayStatValue, 'id' | 'value'>[]; lang: Lang; damage?: DamageHeadline }) {
  const tt = bindT(lang);
  const headline = damage ?? { stat: 'CombinedDPS', label: tt('sidebar.combinedDps') };
  const primary = [headline.stat, 'TotalEHP', 'Life'];
  const label = (id: string) => id === headline.stat ? headline.label : id === 'TotalDPS' ? tt('sidebar.hitDps') : statNameLabel(lang, id);
  const rows = compareStats(before, after);
  const changes = rows.filter(row => row.changed && !primary.includes(row.id));
  const value = (id: string, number: number | null | undefined) => number == null ? '—' : formatStatValue(number, statFormat(id), lang);
  return <div className="swap-gain-card">
    <div className="replacement-metrics">{primary.map(id => {
      const row = rows.find(row => row.id === id);
      return <div className="replacement-metric" data-stat={id} key={id}>
        <span>{label(id)}</span>
        {id === headline.stat && headline.note && <small>{headline.note}</small>}
        <strong className={(row?.delta ?? 0) < 0 ? 'delta-neg' : (row?.delta ?? 0) > 0 ? 'delta-pos' : ''}>
          {row?.percent != null ? `${row.percent > 0 ? '+' : ''}${formatNumber(row.percent, 1, 1)}%` : row ? formatComparisonDelta(row.delta, statFormat(id), lang) : '—'}
        </strong>
        <span><span>{value(id, row?.before)}</span> → <b className="swap-after">{value(id, row?.after)}</b></span>
      </div>;
    })}</div>
    {changes.length > 0 && <details><summary>{tt('items.allChanges').replace('{count}', String(changes.length))}</summary>
      <div className="replacement-table-wrap"><table><thead><tr><th>{tt('items.changedStat')}</th><th>{upgradeT(lang, 'before')}</th><th>{upgradeT(lang, 'after')}</th><th>{upgradeT(lang, 'change')}</th></tr></thead>
        <tbody>{changes.map(row => <tr key={row.id}><th>{label(row.id)}</th><td>{value(row.id, row.before)}</td><td>{value(row.id, row.after)}</td><td>{formatComparisonDelta(row.delta, statFormat(row.id), lang)}</td></tr>)}</tbody>
      </table></div>
    </details>}
  </div>;
}
