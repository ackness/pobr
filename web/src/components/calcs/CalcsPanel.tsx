import { useState } from 'react';
import type { AttributionResponse, Breakdown } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { formatStatValue, statMap } from '../../lib/statDisplay';
import { bindT, type Lang } from '../../lib/i18n';
import { prettySkillId } from '../skills/SkillsPanel';
import './calcs.css';

interface Props {
  session: BuildSession;
  lang: Lang;
}

const MOD_TYPE_ORDER = ['BASE', 'INC', 'MORE', 'OVERRIDE', 'FLAG', 'LIST'];

function BreakdownTable({ name, breakdown, lang }: { name: string; breakdown: Breakdown; lang: Lang }) {
  const tt = bindT(lang);
  return (
    <div className="breakdown-detail">
      <div className="breakdown-summary">
        <span>
          {tt('calcs.baseTotal')}: <strong>{breakdown.base_total}</strong>
        </span>
        <span>
          {tt('calcs.incTotal')}: <strong>{breakdown.inc_total}%</strong>
        </span>
      </div>
      <div className="breakdown-scroll">
        <table className="breakdown-table">
          <thead>
            <tr>
              <th>{tt('calcs.type')}</th>
              <th>{tt('calcs.value')}</th>
              <th>{tt('calcs.modifier')}</th>
              <th>{tt('calcs.source')}</th>
            </tr>
          </thead>
          <tbody>
            {[...breakdown.mods]
              .sort((a, b) => MOD_TYPE_ORDER.indexOf(a.mod_type) - MOD_TYPE_ORDER.indexOf(b.mod_type))
              .map((mod, i) => (
                <tr key={`${name}-${i}`}>
                  <td className={`mod-type mod-type-${mod.mod_type.toLowerCase()}`}>{mod.mod_type}</td>
                  <td className="mod-value">{mod.value ?? '—'}</td>
                  <td className="mod-text">{mod.source_text ?? tt('calcs.baseDerived')}</td>
                  <td className="mod-origin">
                    {mod.origin_kind ? `${mod.origin_kind}${mod.slot ? ` · ${mod.slot}` : ''}` : '—'}
                  </td>
                </tr>
              ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function AttributionView({ session, lang }: { session: BuildSession; lang: Lang }) {
  const tt = bindT(lang);
  const [report, setReport] = useState<AttributionResponse | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fields = ['TotalDPS', 'Life', 'EnergyShield', 'TotalEHP'];

  const run = async () => {
    setRunning(true);
    setError(null);
    try {
      setReport(await session.runAttribution(fields));
    } catch (err) {
      setError(String(err));
    } finally {
      setRunning(false);
    }
  };

  const label = (kind: string, id: string) => {
    if (kind === 'socket_group') {
      const group = session.build?.socket_groups[Number(id)];
      const skill = group?.gems[0]?.skill_id;
      return `${tt('calcs.group')} ${Number(id) + 1}${skill ? ` · ${prettySkillId(skill)}` : ''}`;
    }
    return id;
  };

  return (
    <section className="attribution-view" aria-labelledby="attribution-heading">
      <h3 id="attribution-heading">{tt('calcs.attribution')}</h3>
      <p className="calcs-hint">
{tt('calcs.attributionHint')}
      </p>
      <button onClick={run} disabled={running || session.busy}>
        {running ? tt('calcs.running') : tt('calcs.runAttribution')}
      </button>
      {error && <div className="calc-error">{error}</div>}
      {report && (
        <div className="breakdown-scroll">
          <table className="breakdown-table attribution-table">
            <thead>
              <tr>
                <th>{tt('calcs.source')}</th>
                {fields.map((f) => (
                  <th key={f}>{f}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              <tr className="attribution-baseline">
                <td>{tt('calcs.baseline')}</td>
                {fields.map((f) => (
                  <td key={f}>{formatStatValue(report.baseline[f] ?? null, 'float2')}</td>
                ))}
              </tr>
              {[...report.entries]
                .sort(
                  (a, b) => Math.abs(b.deltas[fields[0]] ?? 0) - Math.abs(a.deltas[fields[0]] ?? 0),
                )
                .map((entry) => (
                  <tr key={`${entry.kind}-${entry.id}`}>
                    <td>
                      <span className="attribution-kind">{entry.kind}</span> {label(entry.kind, entry.id)}
                    </td>
                    {fields.map((f) => {
                      const delta = entry.deltas[f] ?? 0;
                      const baseline = report.baseline[f] ?? 0;
                      const pct = baseline !== 0 ? (delta / baseline) * 100 : 0;
                      return (
                        <td key={f} className={delta > 0 ? 'delta-pos' : delta < 0 ? 'delta-neg' : ''}>
                          {delta === 0 ? '—' : `${delta > 0 ? '+' : ''}${formatStatValue(delta, 'float2')}`}
                          {Math.abs(pct) >= 0.05 && <span className="delta-pct"> ({pct.toFixed(1)}%)</span>}
                        </td>
                      );
                    })}
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

/** Calcs 页：字段点击展开 breakdown（消费 0.3）+ 归因视图（消费 0.4）。 */
export function CalcsPanel({ session, lang }: Props) {
  const tt = bindT(lang);
  const calc = session.calc;
  const [open, setOpen] = useState<string | null>(null);

  if (!calc) return null;
  const values = statMap(calc.stats);
  const breakdownNames = Object.keys(calc.breakdowns);

  return (
    <section aria-labelledby="calcs-heading">
      <h2 id="calcs-heading" className="panel-heading">
        {tt('calcs.title')}
      </h2>
      <p className="calcs-hint">
        {tt('calcs.hint')}
      </p>
      <div className="breakdown-list">
        {breakdownNames.map((name) => {
          const breakdown = calc.breakdowns[name];
          const isOpen = open === name;
          return (
            <div key={name} className={`breakdown-item${isOpen ? ' is-open' : ''}`}>
              <button
                className="breakdown-toggle"
                aria-expanded={isOpen}
                onClick={() => setOpen(isOpen ? null : name)}
              >
                <span className="breakdown-name">{name}</span>
                <span className="breakdown-value">
                  {values.has(name) ? formatStatValue(values.get(name) ?? null, 'float2') : ''}
                </span>
                <span className="breakdown-count">
                  {breakdown.mods.length} {tt('calcs.mods')}
                </span>
              </button>
              {isOpen && <BreakdownTable name={name} breakdown={breakdown} lang={lang} />}
            </div>
          );
        })}
      </div>
      <AttributionView session={session} lang={lang} />
    </section>
  );
}
