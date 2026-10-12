import { PageHeader } from '../shared/PageHeader';
import { formatApiError } from '../../api/error';
import { useMemo, useRef, useState } from 'react';
import type {
  AttributionResponse,
  Breakdown,
  BreakdownMod,
  FormulaStep,
  HitDamageTable,
  HitMitigationRow,
  SkillDotRow,
  DisplayStatCategory,
  SkillMetricsResponse,
} from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { formatStatValue, statMap } from '../../lib/statDisplay';
import {
  bindT,
  damageTypeLabel,
  grantedSourceLabel,
  originKindLabel,
  slotLabel,
  statCategoryLabel,
  statNameLabel,
  type Lang,
} from '../../lib/i18n';
import { getBackend } from '../../api/backend';
import { useEffect } from 'react';
import { useSkillName } from '../../hooks/useSkillName';
import './calcs.css';
import { highestDpsSkill } from '../../lib/skillSelection';

interface Props {
  session: BuildSession;
  lang: Lang;
  /** 侧边栏点击跳转：要展开的 breakdown（对象引用每次点击新建，作为 effect 触发键）。 */
  focus?: { id: string } | null;
  /** 跳转消费完毕的回调（父级清空 focus，防止页签切回时重复触发滚动）。 */
  onFocusConsumed?: () => void;
}

const MOD_TYPE_ORDER = ['BASE', 'INC', 'MORE', 'OVERRIDE', 'FLAG', 'LIST'];

/** 数字统一格式：千分位 + 最多 2 位小数（清掉浮点尾巴）。 */
function fmtNum(value: number): string {
  return value.toLocaleString('en-US', { maximumFractionDigits: 2 });
}

/** 词条展示清洗：剥 `[A|B]` 内部标注。 */
function cleanModText(text: string): string {
  return text
    .replace(/\[([^\]|]*)\|([^\]]*)\]/g, '$2')
    .replace(/\[([^\]]*)\]/g, '$1');
}

/** How a formula step changes the running value, e.g. `+120`, `×1.35 (+35%)`, `≤ 75`. */
function formulaOperand(step: FormulaStep): string {
  const n = fmtNum(step.operand);
  const signed = step.operand < 0 ? `−${fmtNum(-step.operand)}` : `+${n}`;
  switch (step.op) {
    case 'base':
    case 'bonus_to_multiplier':
    case 'round':
    case 'accuracy_rating':
    case 'always_hits':
    case 'non_crit_hit':
    case 'average_hit':
    case 'skill_dot':
    case 'hit_dps':
      return '';
    case 'crit_hits':
      return `${n}%`;
    case 'chance_to_hit':
      return `×${n}%`;
    case 'enemy_mitigation':
    case 'uses_per_second':
    case 'dps_multiplier':
      return `×${n}`;
    case 'vs_evasion':
      return `A × 1.25 ÷ (A + ${n} × 0.3)`;
    case 'flat':
    case 'enemy_flat':
    case 'gear_adjusted':
    case 'gained':
    case 'slot_scaled':
    case 'flat_after':
    case 'poison_dps':
    case 'ignite_dps':
    case 'bleed_dps':
    case 'dot_dps':
      return signed;
    case 'increased':
    case 'enemy_increased':
    case 'inevitable_penalty':
    case 'bifurcate_bonus':
      return `×${fmtNum(1 + step.operand / 100)} (${signed}%)`;
    case 'hit_chance':
      return `×${n}%`;
    case 'lucky':
    case 'bifurcate':
      return '1 − (1 − p)²';
    case 'inevitable':
      return `= ${n}`;
    case 'converted':
    case 'enemy_block':
      return `−${n}%`;
    case 'more':
    case 'action_speed':
      return `×${n}`;
    case 'added_time':
      return `${signed}s`;
    case 'override':
      return `= ${n}`;
    case 'cap':
    case 'cooldown_cap':
    case 'server_tick_cap':
      return `≤ ${n}`;
    case 'floor':
      return `≥ ${n}`;
  }
}

/** The per-type table behind the average hit: base range, scaling, and both averages. */
function HitDamageView({ damage, lang }: { damage: HitDamageTable; lang: Lang }) {
  const tt = bindT(lang);
  const range = (min: number, max: number) => `${fmtNum(min)} – ${fmtNum(max)}`;
  const sum = (key: 'avg' | 'crit_avg') => damage.rows.reduce((total, row) => total + row[key], 0);
  return (
    <div className="breakdown-scroll">
      <table className="breakdown-table hit-damage-table" aria-label={tt('calcs.damage.title')}>
        <caption>{tt('calcs.damage.title')}</caption>
        <thead>
          <tr>
            <th scope="col">{tt('calcs.damage.type')}</th>
            <th scope="col">{tt('calcs.damage.base')}</th>
            <th scope="col">{tt('calcs.damage.increased')}</th>
            <th scope="col">{tt('calcs.damage.more')}</th>
            <th scope="col">{tt('calcs.damage.hit')}</th>
            <th scope="col">{tt('calcs.damage.avg')}</th>
            <th scope="col">{tt('calcs.damage.critAvg')}</th>
          </tr>
        </thead>
        <tbody>
          {damage.rows.map((row) => (
            <tr key={row.damage_type} className={`damage-${row.damage_type.toLowerCase()}`}>
              <th scope="row">{damageTypeLabel(lang, row.damage_type)}</th>
              <td className="mod-value">{range(row.base_min, row.base_max)}</td>
              <td className="mod-value">{fmtNum(row.increased)}%</td>
              <td className="mod-value">
                ×{fmtNum(row.more)}
                {row.more_min !== 1 && ` · ${tt('calcs.damage.minOnly')} ×${fmtNum(row.more_min)}`}
                {row.more_max !== 1 && ` · ${tt('calcs.damage.maxOnly')} ×${fmtNum(row.more_max)}`}
              </td>
              <td className="mod-value">{range(row.min, row.max)}</td>
              <td className="mod-value">{fmtNum(row.avg)}</td>
              <td className="mod-value">{fmtNum(row.crit_avg)}</td>
            </tr>
          ))}
        </tbody>
        <tfoot>
          <tr>
            <th scope="row" colSpan={5}>{tt('calcs.damage.total')}</th>
            <td className="mod-value">{fmtNum(sum('avg'))}</td>
            <td className="mod-value">{fmtNum(sum('crit_avg'))}</td>
          </tr>
        </tfoot>
      </table>
      {damage.damage_effect !== 1 && (
        <p className="breakdown-note">{tt('calcs.damage.effect')}: ×{fmtNum(damage.damage_effect)}</p>
      )}
    </div>
  );
}

/** The lines behind each damage type's increased and more totals: shared ones first, then per type. */
function DamageLinesView({ damage, lang }: { damage: HitDamageTable; lang: Lang }) {
  const tt = bindT(lang);
  const groups = [
    { key: 'shared', label: tt('calcs.damage.sharedLines'), mods: damage.shared_mods },
    ...damage.rows.map((row) => ({
      key: row.damage_type,
      label: `${damageTypeLabel(lang, row.damage_type)} · ${tt('calcs.damage.typeLines')}`,
      mods: row.mods,
    })),
  ].filter((group) => group.mods.length > 0);
  if (groups.length === 0) return null;
  return (
    <div className="damage-lines">
      {groups.map((group) => (
        <details key={group.key} className={`damage-lines-group damage-lines-${group.key.toLowerCase()}`}>
          <summary>
            {group.label}
            <span className="breakdown-count">
              {group.mods.filter((m) => m.active).length} {tt('calcs.mods')}
            </span>
          </summary>
          <ModLines mods={group.mods} lang={lang} />
        </details>
      ))}
    </div>
  );
}

/** What the enemy does to each damage type of the average hit. */
function MitigationView({ rows, lang }: { rows: HitMitigationRow[]; lang: Lang }) {
  const tt = bindT(lang);
  const sum = (key: 'average' | 'mitigated') => rows.reduce((total, row) => total + row[key], 0);
  // Armour depends on the hit size, so the listed physical reduction is the non-critical one.
  const critsDiffer = rows.some(
    (row) => Math.abs(row.multiplier - row.taken * (1 - row.reduction / 100)) > 1e-6,
  );
  return (
    <div className="breakdown-scroll">
      <table className="breakdown-table hit-damage-table mitigation-table" aria-label={tt('calcs.mitigation.title')}>
        <caption>{tt('calcs.mitigation.title')}</caption>
        <thead>
          <tr>
            <th scope="col">{tt('calcs.damage.type')}</th>
            <th scope="col">{tt('calcs.mitigation.average')}</th>
            <th scope="col">{tt('calcs.mitigation.resist')}</th>
            <th scope="col">{tt('calcs.mitigation.reduction')}</th>
            <th scope="col">{tt('calcs.mitigation.taken')}</th>
            <th scope="col">{tt('calcs.mitigation.multiplier')}</th>
            <th scope="col">{tt('calcs.mitigation.after')}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.damage_type} className={`damage-${row.damage_type.toLowerCase()}`}>
              <th scope="row">{damageTypeLabel(lang, row.damage_type)}</th>
              <td className="mod-value">{fmtNum(row.average)}</td>
              <td className="mod-value">{row.resist === null ? tt('calcs.mitigation.armour') : `${fmtNum(row.resist)}%`}</td>
              <td className="mod-value">{fmtNum(row.reduction)}%</td>
              <td className="mod-value">×{fmtNum(row.taken)}</td>
              <td className="mod-value">×{fmtNum(row.multiplier)}</td>
              <td className="mod-value">{fmtNum(row.mitigated)}</td>
            </tr>
          ))}
        </tbody>
        <tfoot>
          <tr>
            <th scope="row">{tt('calcs.damage.total')}</th>
            <td className="mod-value">{fmtNum(sum('average'))}</td>
            <td colSpan={4} />
            <td className="mod-value">{fmtNum(sum('mitigated'))}</td>
          </tr>
        </tfoot>
      </table>
      {critsDiffer && <p className="breakdown-note">{tt('calcs.mitigation.critNote')}</p>}
    </div>
  );
}

/** The skill's own damage over time per type, with the lines that scale each type. */
function SkillDotView({ rows, lang }: { rows: SkillDotRow[]; lang: Lang }) {
  const tt = bindT(lang);
  const showAura = rows.some((row) => row.aura !== 1);
  return (
    <>
      <div className="breakdown-scroll">
        <table className="breakdown-table hit-damage-table skill-dot-table" aria-label={tt('calcs.skillDot.title')}>
          <caption>{tt('calcs.skillDot.title')}</caption>
          <thead>
            <tr>
              <th scope="col">{tt('calcs.damage.type')}</th>
              <th scope="col">{tt('calcs.skillDot.base')}</th>
              <th scope="col">{tt('calcs.damage.increased')}</th>
              <th scope="col">{tt('calcs.damage.more')}</th>
              <th scope="col">{tt('calcs.skillDot.multiplier')}</th>
              {showAura && <th scope="col">{tt('calcs.skillDot.aura')}</th>}
              <th scope="col">{tt('calcs.skillDot.enemy')}</th>
              <th scope="col">{tt('calcs.skillDot.total')}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.damage_type} className={`damage-${row.damage_type.toLowerCase()}`}>
                <th scope="row">{damageTypeLabel(lang, row.damage_type)}</th>
                <td className="mod-value">{fmtNum(row.base)}</td>
                <td className="mod-value">{fmtNum(row.increased)}%</td>
                <td className="mod-value">×{fmtNum(row.more)}</td>
                <td className="mod-value">{fmtNum(row.dot_multiplier)}%</td>
                {showAura && <td className="mod-value">×{fmtNum(row.aura)}</td>}
                <td className="mod-value">×{fmtNum(row.enemy)}</td>
                <td className="mod-value">{fmtNum(row.total)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="damage-lines">
        {rows.filter((row) => row.mods.length > 0).map((row) => (
          <details key={row.damage_type} className={`damage-lines-group damage-lines-${row.damage_type.toLowerCase()}`}>
            <summary>
              {damageTypeLabel(lang, row.damage_type)} · {tt('calcs.skillDot.lines')}
              <span className="breakdown-count">
                {row.mods.filter((m) => m.active).length} {tt('calcs.mods')}
              </span>
            </summary>
            <ModLines mods={row.mods} lang={lang} />
          </details>
        ))}
      </div>
    </>
  );
}

/** Modifier lines with type, applied value, text and source; lines the calculation skips are dimmed. */
function ModLines({ mods, lang }: { mods: BreakdownMod[]; lang: Lang }) {
  const tt = bindT(lang);
  // 中文界面：词条行批量反查翻译（清洗标注后送翻译器；结果缓存本组件）。
  const [zhLines, setZhLines] = useState<Record<string, string>>({});
  useEffect(() => {
    if (lang === 'en-US') return;
    const pending = [
      ...new Set(
        mods
          .map((m) => m.source_text)
          .filter((t): t is string => !!t)
          .map(cleanModText),
      ),
    ];
    if (pending.length === 0) return;
    let cancelled = false;
    getBackend()
      .then((b) => b.translateLines(pending))
      .then((translated) => {
        if (cancelled) return;
        const map: Record<string, string> = {};
        pending.forEach((en, i) => {
          if (translated[i] !== en) map[en] = translated[i];
        });
        setZhLines(map);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [mods, lang]);

  const modText = (raw: string | null) => {
    if (!raw) return tt('calcs.baseDerived');
    const cleaned = cleanModText(raw);
    return lang !== 'en-US' ? (zhLines[cleaned] ?? cleaned) : cleaned;
  };
  return (
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
          {[...mods]
            .sort(
              (a, b) =>
                MOD_TYPE_ORDER.indexOf(a.mod_type) - MOD_TYPE_ORDER.indexOf(b.mod_type) ||
                Number(b.active) - Number(a.active),
            )
            .map((mod, i) => (
              <tr key={i} className={mod.active ? undefined : 'is-inactive'}>
                <td className={`mod-type mod-type-${mod.mod_type.toLowerCase()}`}>{mod.mod_type}</td>
                <td className="mod-value">{mod.value === null ? '—' : fmtNum(mod.value)}</td>
                <td className="mod-text">
                  {modText(mod.source_text)}
                  {!mod.active && <span className="mod-inactive-tag">{tt('calcs.inactive')}</span>}
                </td>
                <td className="mod-origin">
                  {mod.origin_kind
                    ? `${originKindLabel(lang, mod.origin_kind)}${
                        mod.slot ? ` · ${slotLabel(lang, mod.slot)}` : ''
                      }`
                    : '—'}
                </td>
              </tr>
            ))}
        </tbody>
      </table>
    </div>
  );
}

function BreakdownTable({ breakdown, lang }: { breakdown: Breakdown; lang: Lang }) {
  const tt = bindT(lang);
  const damageLines = [
    ...(breakdown.damage ? [breakdown.damage.shared_mods, ...breakdown.damage.rows.map((row) => row.mods)] : []),
    ...(breakdown.skill_dot ?? []).map((row) => row.mods),
  ].flat();
  return (
    <div className="breakdown-detail">
      {breakdown.damage && <HitDamageView damage={breakdown.damage} lang={lang} />}
      {breakdown.skill_dot && <SkillDotView rows={breakdown.skill_dot} lang={lang} />}
      {breakdown.formula ? (
        <table className="breakdown-formula" aria-label={tt('calcs.formula')}>
          <caption>{tt('calcs.formula')}</caption>
          <tbody>
            {breakdown.formula.steps.map((step, i) => (
              <tr key={i} className={`formula-step formula-${step.op}`}>
                <th scope="row">{tt(`calcs.step.${step.op}`)}</th>
                <td className="formula-operand">{formulaOperand(step)}</td>
                <td className="formula-result">{fmtNum(step.result)}</td>
              </tr>
            ))}
            <tr className="formula-final">
              <th scope="row">{tt('calcs.formulaResult')}</th>
              <td />
              <td className="formula-result">{fmtNum(breakdown.formula.value)}</td>
            </tr>
          </tbody>
        </table>
      ) : (
        <>
          <div className="breakdown-summary">
            <span>
              {tt('calcs.baseTotal')}: <strong>{fmtNum(breakdown.base_total)}</strong>
            </span>
            <span>
              {tt('calcs.incTotal')}: <strong>{fmtNum(breakdown.inc_total)}%</strong>
            </span>
            {breakdown.more_total !== 1 && (
              <span>
                {tt('calcs.moreTotal')}: <strong>×{fmtNum(breakdown.more_total)}</strong>
              </span>
            )}
          </div>
          <p className="breakdown-note breakdown-totals-note">{tt('calcs.totalsNote')}</p>
        </>
      )}
      {breakdown.mitigation && breakdown.mitigation.length > 0 && (
        <MitigationView rows={breakdown.mitigation} lang={lang} />
      )}
      {[...breakdown.mods, ...damageLines].some((m) => !m.active) && (
        <p className="breakdown-note">{tt('calcs.inactiveNote')}</p>
      )}
      {breakdown.damage && <DamageLinesView damage={breakdown.damage} lang={lang} />}
      {breakdown.mods.length > 0 && <ModLines mods={breakdown.mods} lang={lang} />}
    </div>
  );
}

/** Every active effect is calculated independently; selection only changes inspection context. */
function SkillMetricsView({ session, lang }: { session: BuildSession; lang: Lang }) {
  const tt = bindT(lang);
  const [completed, setCompleted] = useState<{ version: number; report: SkillMetricsResponse } | null>(null);
  const [running, setRunning] = useState(false);
  const [failure, setFailure] = useState<{ version: number; message: string } | null>(null);
  const { stateVersion, runSkillMetrics } = session;
  const report = completed?.version === stateVersion ? completed.report : null;
  const displayed = completed?.report;
  const error = failure?.version === stateVersion ? failure.message : null;
  useEffect(() => {
    let cancelled = false;
    setRunning(true);
    const timer = setTimeout(() => {
      setFailure(null);
      runSkillMetrics()
        .then((result) => {
          if (!cancelled) setCompleted({ version: stateVersion, report: result });
        })
        .catch((err) => {
          if (!cancelled) setFailure({ version: stateVersion, message: formatApiError(err) });
        })
        .finally(() => {
          if (!cancelled) setRunning(false);
        });
    }, 250);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [stateVersion, runSkillMetrics]);

  const skillName = useSkillName(lang);
  const mainIndex = session.calcParams.main_socket_group;
  const highest = report ? highestDpsSkill(report.skills, session.socketGroups) : null;
  const columns = [
    ['combined_dps', 'sidebar.combinedDps'],
    ['average_hit', 'calcs.averageHit'],
    ['action_rate', 'calcs.actionRate'],
    ['mana_cost', 'calcs.manaCost'],
    ['life_cost', 'calcs.lifeCost'],
    ['cooldown', 'calcs.cooldown'],
  ] as const;

  return (
    <section className="ui-card attribution-view" aria-labelledby="skill-metrics-heading" aria-busy={running}>
      <h3 id="skill-metrics-heading" tabIndex={-1}>{tt('calcs.skillMetrics')}</h3>
      <p className="calcs-hint">{tt('calcs.skillMetricsHint')}</p>
      {running && <p className="calcs-hint" role="status">{tt('calcs.running')}</p>}
      {error && <div className="calc-error" role="alert">{error}</div>}
      {report?.item_errors.map((issue, index) => (
        <p className="calc-error" key={`${issue.slot}-${index}`}>{slotLabel(lang, issue.slot)}: {issue.message}</p>
      ))}
      {displayed && (displayed.skills.length === 0 ? (
        <p className="calcs-hint">{tt('calcs.skillMetricsEmpty')}</p>
      ) : (
        <div className="breakdown-scroll">
          <table className="breakdown-table skill-metrics-table">
            <thead>
              <tr>
                <th scope="col">{tt('calcs.skill')}</th>
                {columns.map(([key, label]) => <th scope="col" key={key}>{tt(label)}</th>)}
                <th scope="col">{tt('calcs.diagnostics')}</th>
              </tr>
            </thead>
            <tbody>
              {displayed.skills.map(entry => {
                const group = session.socketGroups[entry.group_index];
                const granted = grantedSourceLabel(lang, group?.source);
                const selected = entry.group_index === mainIndex
                  && entry.active_skill_index === (group?.main_active_skill ?? 1);
                const available = !!report && entry.status === 'calculated' && entry.metrics !== null;
                const disabled = !report || session.busy || !group?.enabled;
                const inspect = () => {
                  if (!disabled) session.inspectSkill(entry.group_index, entry.active_skill_index, stateVersion);
                };
                return (
                  <tr key={`${entry.group_index}-${entry.active_skill_index}`}
                    className={`skill-metrics-row${selected ? ' is-main' : ''}${granted ? ' is-granted' : ''}`}
                    title={group?.gems.map(gem => skillName(gem.skill_id)).join(' + ')}
                    onClick={inspect}>
                    <td>
                      <button className="calcs-skill-button" aria-pressed={selected}
                        aria-disabled={disabled}
                        onClick={event => { event.stopPropagation(); inspect(); }}>
                        {tt('calcs.group')} {entry.group_index + 1} · {entry.active_skill_index} · {skillName(entry.skill_id)}
                      </button>
                      {granted && <span className="granted-badge">{granted}</span>}
                      {entry === highest && entry.metrics!.combined_dps > 0 && <span className="granted-badge">{tt('calcs.highestDps')}</span>}
                    </td>
                    {columns.map(([key]) => (
                      <td className="mod-value" key={key}>{available ? formatStatValue(entry.metrics![key], 'float2') : '—'}</td>
                    ))}
                    <td className="skill-metrics-diagnostics">
                      {report && !available && <p className="calc-error">{tt('calcs.unavailable')}: {entry.error || tt('calcs.unavailableHint')}</p>}
                      {available && entry.error && <p className="calc-error">{entry.error}</p>}
                      {report && entry.unsupported_modifiers.length > 0 && (
                        <details onClick={event => event.stopPropagation()}>
                          <summary>{tt('calcs.unsupported')} ({entry.unsupported_modifiers.length})</summary>
                          <ul>{entry.unsupported_modifiers.map((modifier, index) => <li key={index}>{modifier}</li>)}</ul>
                        </details>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      ))}
    </section>
  );
}

function AttributionView({ session, lang }: { session: BuildSession; lang: Lang }) {
  const tt = bindT(lang);
  const [completed, setCompleted] = useState<{ version: number; report: AttributionResponse } | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fields = ['TotalDPS', 'Life', 'EnergyShield', 'TotalEHP'];
  const sequence = useRef(0);
  const report = completed?.version === session.stateVersion ? completed.report : null;
  useEffect(() => {
    sequence.current++;
    setRunning(false);
    setError(null);
    return () => { sequence.current++; };
  }, [session.stateVersion]);

  const run = async () => {
    if (session.busy || !session.calc) return;
    const current = ++sequence.current;
    const version = session.stateVersion;
    setRunning(true);
    setError(null);
    try {
      const result = await session.runAttribution(fields);
      if (current === sequence.current) setCompleted({ version, report: result });
    } catch (err) {
      if (current === sequence.current) setError(formatApiError(err));
    } finally {
      if (current === sequence.current) setRunning(false);
    }
  };

  const skillName = useSkillName(lang);
  const label = (kind: string, id: string) => {
    if (kind === 'socket_group') {
      const group = session.socketGroups[Number(id)];
      const gems = group?.gems.map(gem =>
        gem.skill_id ? skillName(gem.skill_id) : gem.name_spec || tt('skills.unresolved')).join(' + ');
      return `${tt('calcs.group')} ${Number(id) + 1}${gems ? ` · ${gems}` : ''}`;
    }
    if (kind === 'item') return slotLabel(lang, id);
    if (kind === 'flask') return slotLabel(lang, id);
    return id;
  };

  return (
    <section className="ui-card attribution-view" aria-labelledby="attribution-heading">
      <h3 id="attribution-heading" tabIndex={-1}>{tt('calcs.attribution')}</h3>
      <p className="calcs-hint">
{tt('calcs.attributionHint')}
      </p>
      <button onClick={run} disabled={running || session.busy || !session.calc}>
        {running ? tt('calcs.running') : tt('calcs.runAttribution')}
      </button>
      {completed && !report && <p className="calcs-hint" role="status">{tt('calcs.attributionStale')}</p>}
      {error && <div className="calc-error">{error}</div>}
      {report && (
        <div className="breakdown-scroll">
          <table className="breakdown-table attribution-table">
            <thead>
              <tr>
                <th>{tt('calcs.source')}</th>
                {fields.map((f) => (
                  <th key={f}>{statNameLabel(lang, f)}</th>
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
export function CalcsPanel({ session, lang, focus, onFocusConsumed }: Props) {
  const tt = bindT(lang);
  const calc = session.calcError ? null : session.calc;
  const [open, setOpen] = useState<string | null>(null);

  // 侧边栏跳转：展开目标 breakdown 并滚动到可视区（渲染提交后再滚）。
  // 消费后立即让父级清空 focus——否则切走再切回本页签会带着旧 focus 重新
  // 挂载，scrollIntoView 再次触发，页面整体错位。
  useEffect(() => {
    if (!focus) return;
    const overview = focus.id === 'skill-metrics';
    if (!overview) setOpen(focus.id);
    const frame = requestAnimationFrame(() => {
      const target = document.getElementById(overview ? 'skill-metrics-heading' : `breakdown-${focus.id}`);
      if (overview) target?.focus({ preventScroll: true });
      target?.scrollIntoView({ block: 'start', behavior: 'smooth' });
      onFocusConsumed?.();
    });
    return () => cancelAnimationFrame(frame);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focus]);

  const [query, setQuery] = useState('');

  // 分节：按 display_catalog 分类分组全部聚合量（不在目录里的进「其他聚合量」），
  // 组内保持 stats 的目录顺序，目录外条目按名称排序垫底。
  const sections = useMemo(() => {
    if (!calc) return [];
    const catOf = new Map<string, DisplayStatCategory>(calc.stats.map((s) => [s.id, s.category]));
    const catalogOrder = new Map<string, number>(calc.stats.map((s, i) => [s.id, i]));
    const q = query.trim().toLowerCase();
    const names = Object.keys(calc.breakdowns)
      .filter(
        (name) =>
          q === '' ||
          name.toLowerCase().includes(q) ||
          statNameLabel(lang, name).toLowerCase().includes(q),
      )
      .sort((a, b) => {
        const ia = catalogOrder.get(a) ?? Number.MAX_SAFE_INTEGER;
        const ib = catalogOrder.get(b) ?? Number.MAX_SAFE_INTEGER;
        return ia !== ib ? ia - ib : a.localeCompare(b);
      });
    const byCat = new Map<string, string[]>();
    for (const name of names) {
      const cat = catOf.get(name) ?? 'Other';
      if (!byCat.has(cat)) byCat.set(cat, []);
      byCat.get(cat)!.push(name);
    }
    const CAT_ORDER: string[] = [
      'Offence', 'HitDamage', 'DotDamage', 'Ailment', 'SkillMechanics',
      'Defence', 'Resistance', 'Avoidance', 'Mitigation',
      'Resource', 'Recovery', 'Degen', 'Cost', 'Requirement', 'Minion', 'Utility', 'Other',
    ];
    return CAT_ORDER.filter((c) => byCat.has(c)).map((category) => ({
      category,
      names: byCat.get(category)!,
    }));
  }, [calc, query, lang]);

  const values = statMap(calc?.stats ?? []);

  return (
    <section className="ui-page calcs-page" aria-labelledby="calcs-heading">
      <PageHeader id="calcs-heading" title={tt('calcs.title')} description={tt('calcs.hint')} />
      <div className="calcs-tools">
      <nav className="calcs-navigation" aria-label={tt('calcs.title')}>
        {(['calcs.skillMetrics', 'calcs.attribution'] as const).filter(key => key === 'calcs.skillMetrics' || calc).map((key, index) => <button key={key} onClick={() => {
          const section = document.getElementById(index === 0 ? 'skill-metrics-heading' : 'attribution-heading');
          section?.focus({ preventScroll: true });
          section?.scrollIntoView({ block: 'start' });
        }}>{tt(key)} ↓</button>)}
      </nav>
      <input
        className="calcs-search"
        type="search"
        placeholder={tt('calcs.search')}
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        aria-label={tt('calcs.search')}
      />
      </div>
      <SkillMetricsView session={session} lang={lang} />
      <div className="calcs-breakdowns">
      {query.trim() && sections.length === 0 && <div className="search-empty" role="status">
        <p>{tt('common.noResults')}</p>
        <button onClick={() => setQuery('')}>{tt('common.clearSearch')}</button>
      </div>}
      {sections.map(({ category, names }) => (
        <section key={category} className="calcs-section">
          <h3 className="calcs-section-title">{statCategoryLabel(lang, category)}</h3>
          <div className="breakdown-list">
            {names.map((name) => {
              const breakdown = calc!.breakdowns[name];
              const isOpen = open === name;
              return (
                <div
                  key={name}
                  id={`breakdown-${name}`}
                  className={`breakdown-item${isOpen ? ' is-open' : ''}`}
                >
                  <button
                    className="breakdown-toggle"
                    aria-expanded={isOpen}
                    onClick={() => setOpen(isOpen ? null : name)}
                  >
                    <span className="row-caret" aria-hidden>▸</span>
                    <span className="breakdown-name">{statNameLabel(lang, name)}</span>
                    <span className="breakdown-value">
                      {values.has(name) ? formatStatValue(values.get(name) ?? null, 'float2') : ''}
                    </span>
                    {breakdown.mods.length > 0 && (
                      <span className="breakdown-count">
                        {breakdown.mods.filter((m) => m.active).length} {tt('calcs.mods')}
                      </span>
                    )}
                  </button>
                  {isOpen && <BreakdownTable breakdown={breakdown} lang={lang} />}
                </div>
              );
            })}
          </div>
        </section>
      ))}
      </div>
      <AttributionView session={session} lang={lang} />
    </section>
  );
}
