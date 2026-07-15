import { formatApiError } from '../../api/error';
import { useMemo, useState } from 'react';
import type {
  AttributionResponse,
  Breakdown,
  DisplayStatCategory,
  FullDpsResponse,
} from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { formatStatValue, statMap } from '../../lib/statDisplay';
import {
  bindT,
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

function BreakdownTable({ name, breakdown, lang }: { name: string; breakdown: Breakdown; lang: Lang }) {
  const tt = bindT(lang);
  // 中文界面：词条行批量反查翻译（清洗标注后送翻译器；结果缓存本组件）。
  const [zhLines, setZhLines] = useState<Record<string, string>>({});
  useEffect(() => {
    if (lang === 'en-US') return;
    const pending = [
      ...new Set(
        breakdown.mods
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
  }, [breakdown, lang]);

  const modText = (raw: string | null) => {
    if (!raw) return tt('calcs.baseDerived');
    const cleaned = cleanModText(raw);
    return lang !== 'en-US' ? (zhLines[cleaned] ?? cleaned) : cleaned;
  };
  return (
    <div className="breakdown-detail">
      <div className="breakdown-summary">
        <span>
          {tt('calcs.baseTotal')}: <strong>{fmtNum(breakdown.base_total)}</strong>
        </span>
        <span>
          {tt('calcs.incTotal')}: <strong>{fmtNum(breakdown.inc_total)}%</strong>
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
                  <td className="mod-value">{mod.value === null ? '—' : fmtNum(mod.value)}</td>
                  <td className="mod-text">{modText(mod.source_text)}</td>
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
    </div>
  );
}

/**
 * 逐技能组 DPS（PoB2 侧栏技能列表的 Calcs 版）：每组走完整管线 scoped 重算
 * （品质/升华/装备词条全生效）。挂载与 build 变动时自动刷新（轻度防抖合并
 * 连续编辑）；点击某行把该组设为主技能。
 */
function FullDpsView({ session, lang }: { session: BuildSession; lang: Lang }) {
  const tt = bindT(lang);
  const [report, setReport] = useState<FullDpsResponse | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const { stateVersion, runFullDps } = session;
  useEffect(() => {
    let cancelled = false;
    const timer = setTimeout(() => {
      setRunning(true);
      setError(null);
      runFullDps()
        .then((result) => {
          if (!cancelled) setReport(result);
        })
        .catch((err) => {
          if (!cancelled) setError(formatApiError(err));
        })
        .finally(() => {
          if (!cancelled) setRunning(false);
        });
    }, 250);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [stateVersion, runFullDps]);

  const skillName = useSkillName(lang);
  const mainIndex = session.calc?.main_skill?.group_index;
  const groupLabel = (index: number, skillId: string) =>
    `${tt('calcs.group')} ${index + 1} · ${skillName(skillId)}`;
  // 同名技能可合法出现多次（玩家备用组 + 装备/天赋附赠组）；徽标标出附赠来源，
  // tooltip 列出整组宝石链帮助区分玩家自己的重复组。
  const groupMeta = (index: number) => {
    const group = session.socketGroups[index];
    return {
      granted: grantedSourceLabel(lang, group?.source),
      gems: (group?.gems ?? []).map((g) => skillName(g.skill_id)).join(' + '),
    };
  };

  return (
    <section className="attribution-view" aria-labelledby="fulldps-heading">
      <h3 id="fulldps-heading">
        {tt('calcs.fullDps')}
        {running && <span className="calcs-hint"> {tt('calcs.running')}</span>}
      </h3>
      <p className="calcs-hint">{tt('calcs.fullDpsHint')}</p>
      {error && <div className="calc-error">{error}</div>}
      {report &&
        (report.per_skill.length === 0 ? (
          <p className="calcs-hint">{tt('calcs.fullDpsEmpty')}</p>
        ) : (
          <div className="breakdown-scroll">
            <table className="breakdown-table">
              <thead>
                <tr>
                  <th>{tt('calcs.skill')}</th>
                  <th>DPS</th>
                </tr>
              </thead>
              <tbody>
                {[...report.per_skill]
                  .sort((a, b) => b.dps - a.dps)
                  .map((entry) => {
                    const meta = groupMeta(entry.group_index);
                    return (
                      <tr
                        key={entry.group_index}
                        className={`fulldps-row${entry.group_index === mainIndex ? ' is-main' : ''}${meta.granted ? ' is-granted' : ''}`}
                        title={meta.gems || tt('skills.setMain')}
                        onClick={() =>
                          session.updateParams({ main_socket_group: entry.group_index })
                        }
                      >
                        <td>
                          {groupLabel(entry.group_index, entry.skill_id)}
                          {meta.granted && (
                            <span className="granted-badge">{meta.granted}</span>
                          )}
                        </td>
                        <td className="mod-value">{formatStatValue(entry.dps, 'float2')}</td>
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
      setError(formatApiError(err));
    } finally {
      setRunning(false);
    }
  };

  const skillName = useSkillName(lang);
  const label = (kind: string, id: string) => {
    if (kind === 'socket_group') {
      const group = session.build?.socket_groups[Number(id)];
      const skill = group?.gems[0]?.skill_id;
      return `${tt('calcs.group')} ${Number(id) + 1}${skill ? ` · ${skillName(skill)}` : ''}`;
    }
    if (kind === 'item') return slotLabel(lang, id);
    if (kind === 'flask') return slotLabel(lang, id);
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
  const calc = session.calc;
  const [open, setOpen] = useState<string | null>(null);

  // 侧边栏跳转：展开目标 breakdown 并滚动到可视区（渲染提交后再滚）。
  // 消费后立即让父级清空 focus——否则切走再切回本页签会带着旧 focus 重新
  // 挂载，scrollIntoView 再次触发，页面整体错位。
  useEffect(() => {
    if (!focus) return;
    setOpen(focus.id);
    requestAnimationFrame(() => {
      document
        .getElementById(`breakdown-${focus.id}`)
        ?.scrollIntoView({ block: 'start', behavior: 'smooth' });
    });
    onFocusConsumed?.();
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

  if (!calc) return null;
  const values = statMap(calc.stats);

  return (
    <section aria-labelledby="calcs-heading">
      <h2 id="calcs-heading" className="panel-heading">
        {tt('calcs.title')}
      </h2>
      <p className="calcs-hint">
        {tt('calcs.hint')}
      </p>
      <input
        className="calcs-search"
        type="search"
        placeholder={tt('calcs.search')}
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        aria-label={tt('calcs.search')}
      />
      {sections.map(({ category, names }) => (
        <section key={category} className="calcs-section">
          <h3 className="calcs-section-title">{statCategoryLabel(lang, category)}</h3>
          <div className="breakdown-list">
            {names.map((name) => {
              const breakdown = calc.breakdowns[name];
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
                    <span className="breakdown-name">{statNameLabel(lang, name)}</span>
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
        </section>
      ))}
      <FullDpsView session={session} lang={lang} />
      <AttributionView session={session} lang={lang} />
    </section>
  );
}
