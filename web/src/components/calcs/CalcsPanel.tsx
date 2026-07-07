import { useState } from 'react';
import type { AttributionResponse, Breakdown } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { formatStatValue, statMap, type Lang } from '../../lib/statDisplay';
import { prettySkillId } from '../skills/SkillsPanel';
import './calcs.css';

interface Props {
  session: BuildSession;
  lang: Lang;
}

const MOD_TYPE_ORDER = ['BASE', 'INC', 'MORE', 'OVERRIDE', 'FLAG', 'LIST'];

function BreakdownTable({ name, breakdown, zh }: { name: string; breakdown: Breakdown; zh: boolean }) {
  return (
    <div className="breakdown-detail">
      <div className="breakdown-summary">
        <span>
          {zh ? '基礎合計' : 'Base total'}: <strong>{breakdown.base_total}</strong>
        </span>
        <span>
          {zh ? '增加合計' : 'Increased total'}: <strong>{breakdown.inc_total}%</strong>
        </span>
      </div>
      <div className="breakdown-scroll">
        <table className="breakdown-table">
          <thead>
            <tr>
              <th>{zh ? '類型' : 'Type'}</th>
              <th>{zh ? '數值' : 'Value'}</th>
              <th>{zh ? '詞條' : 'Modifier'}</th>
              <th>{zh ? '來源' : 'Source'}</th>
            </tr>
          </thead>
          <tbody>
            {[...breakdown.mods]
              .sort((a, b) => MOD_TYPE_ORDER.indexOf(a.mod_type) - MOD_TYPE_ORDER.indexOf(b.mod_type))
              .map((mod, i) => (
                <tr key={`${name}-${i}`}>
                  <td className={`mod-type mod-type-${mod.mod_type.toLowerCase()}`}>{mod.mod_type}</td>
                  <td className="mod-value">{mod.value ?? '—'}</td>
                  <td className="mod-text">{mod.source_text ?? (zh ? '（基底/派生）' : '(base/derived)')}</td>
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

function AttributionView({ session, zh }: { session: BuildSession; zh: boolean }) {
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
      return `${zh ? '技能組' : 'Group'} ${Number(id) + 1}${skill ? ` · ${prettySkillId(skill)}` : ''}`;
    }
    return id;
  };

  return (
    <section className="attribution-view" aria-labelledby="attribution-heading">
      <h3 id="attribution-heading">{zh ? '來源貢獻歸因' : 'Source Attribution'}</h3>
      <p className="calcs-hint">
        {zh
          ? '對每個來源做「移除後重算」，報告其對關鍵字段的邊際貢獻（計算量大，點擊觸發）。'
          : 'Recomputes the build without each source and reports marginal contributions (expensive; click to run).'}
      </p>
      <button onClick={run} disabled={running || session.busy}>
        {running ? (zh ? '歸因計算中…' : 'Running…') : zh ? '計算歸因' : 'Run attribution'}
      </button>
      {error && <div className="calc-error">{error}</div>}
      {report && (
        <div className="breakdown-scroll">
          <table className="breakdown-table attribution-table">
            <thead>
              <tr>
                <th>{zh ? '來源' : 'Source'}</th>
                {fields.map((f) => (
                  <th key={f}>{f}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              <tr className="attribution-baseline">
                <td>{zh ? '基線（完整 build）' : 'Baseline (full build)'}</td>
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
  const zh = lang === 'zh-TW';
  const calc = session.calc;
  const [open, setOpen] = useState<string | null>(null);

  if (!calc) return null;
  const values = statMap(calc.stats);
  const breakdownNames = Object.keys(calc.breakdowns);

  return (
    <section aria-labelledby="calcs-heading">
      <h2 id="calcs-heading" className="panel-heading">
        {zh ? '計算明細' : 'Calculations'}
      </h2>
      <p className="calcs-hint">
        {zh ? '點擊字段展開 base/inc 分解與逐來源詞條。' : 'Click a stat to expand its base/inc decomposition and per-source modifiers.'}
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
                  {breakdown.mods.length} {zh ? '條' : 'mods'}
                </span>
              </button>
              {isOpen && <BreakdownTable name={name} breakdown={breakdown} zh={zh} />}
            </div>
          );
        })}
      </div>
      <AttributionView session={session} zh={zh} />
    </section>
  );
}
