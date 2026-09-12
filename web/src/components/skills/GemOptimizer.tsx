import { formatApiError } from '../../api/error';
import { useEffect, useMemo, useRef, useState } from 'react';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useUpgradeGoal } from '../../hooks/useUpgradeGoal';
import { bindT, type Lang } from '../../lib/i18n';
import { statMap } from '../../lib/statDisplay';
import { supportText } from '../../lib/supportI18n';
import { applySupportPlan, eligibleSupports, lineageAvailable, optimizeSupports, type SupportMetadata, type SupportOptimization } from '../../lib/supportOptimizer';
import { ObjectiveEditor, OptimizerProgress, objectiveOf } from '../shared/OptimizerControls';

interface Props {
  session: BuildSession;
  lang: Lang;
  groupIndex: number;
  catalog: SupportMetadata[];
  gemName: (skillId: string) => string;
  skillKey: string;
  focusNonce?: number;
}

function savedExclusions(key: string): string[] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(key) ?? '[]');
    if (Array.isArray(value)) return [...new Set(value.filter((id): id is string => typeof id === 'string'))];
  } catch { /* Invalid or unavailable storage leaves every candidate enabled. */ }
  return [];
}

export function GemOptimizer({ session, lang, groupIndex, catalog, gemName, skillKey, focusNonce }: Props) {
  const tt = bindT(lang);
  const st = (key: Parameters<typeof supportText>[1]) => supportText(lang, key);
  const { goal, setGoal } = useUpgradeGoal();
  const group = session.socketGroups[groupIndex];
  const byId = useMemo(() => new Map(catalog.map(gem => [gem.skill_id, gem])), [catalog]);
  const occupied = group?.gems.filter(gem => byId.get(gem.skill_id)?.is_support).length ?? 0;
  const [expanded, setExpanded] = useState(false);
  const [capacity, setCapacity] = useState(Math.max(2, Math.min(5, occupied)));
  const [includeLineage, setIncludeLineage] = useState(true);
  const preferenceKey = `pobr-support-exclusions:${skillKey}`;
  const [excludedIds, setExcludedIds] = useState(() => savedExclusions(preferenceKey));
  const excluded = useMemo(() => new Set(excludedIds), [excludedIds]);
  const [search, setSearch] = useState('');
  useEffect(() => setCapacity(value => Math.max(value, Math.min(5, occupied))), [occupied]);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [evaluated, setEvaluated] = useState<(SupportOptimization & { requestKey: string; settingsKey: string }) | null>(null);
  const abortRef = useRef<AbortController | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const pool = useMemo(() => {
    if (!group) return { gems: [], unknown: 0, levelBlocked: 0, incompatible: 0 };
    const eligible = eligibleSupports(group, catalog, session.character?.level ?? 1, includeLineage);
    const gems = eligible.gems.filter(gem => lineageAvailable(gem, session.socketGroups, groupIndex));
    return { ...eligible, gems, incompatible: eligible.incompatible + eligible.gems.length - gems.length };
  }, [group, catalog, session.character?.level, includeLineage, session.socketGroups, groupIndex]);
  const candidateCount = pool.gems.filter(gem => !excluded.has(gem.skill_id)).length;
  const canSearch = candidateCount > 0 || occupied > 0;
  const query = search.trim().toLocaleLowerCase(lang);
  const visibleGems = pool.gems.filter(gem => !query || `${gemName(gem.skill_id)} ${gem.name}`.toLocaleLowerCase(lang).includes(query));
  const settingsKey = JSON.stringify([session.stateVersion, groupIndex, capacity, includeLineage, goal, excludedIds]);
  const result = evaluated?.settingsKey === settingsKey ? evaluated : null;
  const updateExclusions = (next: string[]) => {
    // Invalidate synchronously so a running batch or an old Apply action cannot
    // publish a plan containing a support the player has just excluded.
    abortRef.current?.abort();
    abortRef.current = null;
    setProgress(null);
    setEvaluated(null);
    setError(null);
    setExcludedIds(next);
    try { localStorage.setItem(preferenceKey, JSON.stringify(next)); } catch { /* In-memory filtering still works. */ }
  };
  const exclude = (id: string) => updateExclusions([...new Set([...excludedIds, id])]);
  const restore = (id: string) => updateExclusions(excludedIds.filter(value => value !== id));
  const mainIndex = session.calcParams.main_socket_group ?? session.build?.main_socket_group ?? 0;
  const select = () => {
    if (mainIndex !== groupIndex) session.updateParams({ main_socket_group: groupIndex });
  };
  useEffect(() => {
    if (focusNonce === undefined) return;
    setExpanded(true);
    rootRef.current?.scrollIntoView({ block: 'center', behavior: 'smooth' });
  }, [focusNonce]);
  useEffect(() => {
    abortRef.current?.abort();
    abortRef.current = null;
    setProgress(null);
    setEvaluated(null);
    setError(null);
  }, [settingsKey]);
  useEffect(() => () => abortRef.current?.abort(), []);

  const run = async () => {
    if (mainIndex !== groupIndex) { select(); return; }
    const request = session.currentRequest();
    if (!request || !canSearch) return;
    const controller = new AbortController();
    abortRef.current?.abort();
    abortRef.current = controller;
    const requestKey = JSON.stringify(request);
    setProgress({ done: 0, total: candidateCount });
    setError(null);
    setEvaluated(null);
    try {
      const result = await optimizeSupports({ request, groupIndex, catalog, capacity, includeLineage,
        excludedSkillIds: excludedIds,
        objective: objectiveOf(goal, Object.fromEntries([...statMap(session.calc?.stats ?? [])].map(([key, value]) => [key, value ?? 0]))),
        signal: controller.signal, onProgress: (done, total) => {
          if (!controller.signal.aborted) setProgress({ done, total });
        } });
      if (!controller.signal.aborted && abortRef.current === controller) setEvaluated({ ...result, requestKey, settingsKey });
    } catch (err: unknown) {
      if (!controller.signal.aborted && abortRef.current === controller) setError(formatApiError(err));
    } finally {
      if (abortRef.current === controller) { setProgress(null); abortRef.current = null; }
    }
  };
  const apply = (index: number) => {
    if (!result || result.requestKey !== JSON.stringify(session.currentRequest())) return;
    session.setSocketGroups(applySupportPlan(session.socketGroups, groupIndex, result.plans[index]));
  };
  const number = (value: number) => value.toLocaleString(lang, { maximumFractionDigits: 1 });
  const change = (before: number, after: number) => before > 0
    ? `${after >= before ? '+' : ''}${((after / before - 1) * 100).toFixed(1)}%` : `${after >= before ? '+' : ''}${number(after - before)}`;

  return (
    <div className="gem-optimizer" ref={rootRef}>
      <button className="gem-optimizer-toggle" aria-expanded={expanded} onClick={() => {
        setExpanded(!expanded);
        if (!expanded) select();
      }}>
        <span className="row-caret" aria-hidden>▸</span>{st('title')}
      </button>
      {expanded && <div className="gem-optimizer-body">
        <p className="skills-hint">{st('hint')}</p>
        <div className="support-pool-summary"><strong>{candidateCount}</strong> {st('eligible')}
          <span>{st('excluded')} {pool.unknown} / {pool.levelBlocked} / {pool.incompatible}</span>
        </div>
        <details className="support-pool-details"><summary>{st('filterTitle')}</summary>
          <p className="skills-hint">{st('filterHint')}</p>
          <label className="support-search">{st('search')}<input type="search" value={search}
            placeholder={st('searchPlaceholder')} onChange={event => setSearch(event.target.value)} /></label>
          <div className="opt-candidates">{visibleGems.map(gem => <label key={gem.skill_id}
            className={`opt-chip support-candidate${excluded.has(gem.skill_id) ? ' is-excluded' : ''}`} data-skill-id={gem.skill_id}>
            <input type="checkbox" checked={!excluded.has(gem.skill_id)}
              onChange={event => event.target.checked ? restore(gem.skill_id) : exclude(gem.skill_id)} />
            <span>{gemName(gem.skill_id)}</span>{gem.is_lineage && <span className="gem-lineage-badge">{tt('picker.lineage')}</span>}
          </label>)}</div>
          {!visibleGems.length && <p className="skills-hint">{st('noMatch')}</p>}
        </details>
        {excludedIds.length > 0 && <div className="support-excluded">
          <div className="support-excluded-header"><strong>{st('manualExcluded')} · {excludedIds.length}</strong>
            <button onClick={() => updateExclusions([])}>{st('restoreAll')}</button></div>
          <div className="opt-candidates">{excludedIds.map(id => <button key={id} className="opt-chip"
            aria-label={`${st('restore')} ${gemName(id)}`} title={`${st('restore')} ${gemName(id)}`} onClick={() => restore(id)}>
            {gemName(id)} <span aria-hidden>↶</span>
          </button>)}</div>
          <p className="skills-hint">{st('excludedHint')}</p>
        </div>}
        <div className="opt-row">
          <label>{st('capacity')}<input type="number" min={2} max={5} value={capacity} onChange={event => {
            const value = Number(event.target.value);
            if (Number.isInteger(value) && value >= 2 && value <= 5) setCapacity(value);
          }} /></label>
          <ObjectiveEditor value={goal} onChange={setGoal} lang={lang} />
          <button className="opt-run" disabled={session.busy || progress !== null || !canSearch || !group?.enabled} onClick={run}>
            {progress ? tt('opt.running') : mainIndex !== groupIndex ? st('selectMain') : st('run')}
          </button>
        </div>
        <label className="support-lineage-option"><input type="checkbox" checked={includeLineage}
          onChange={event => setIncludeLineage(event.target.checked)} />{st('lineage')}</label>
        <p className="skills-hint">{st('capacityHint')}</p>
        {progress && <OptimizerProgress {...progress} onCancel={() => abortRef.current?.abort()} lang={lang} />}
        {!pool.gems.length && <p className="skills-hint">{st('empty')}</p>}
        {pool.gems.length > 0 && !candidateCount && <p className="skills-hint">{st(occupied ? 'removeOnly' : 'allExcluded')}</p>}
        {error && <p className="opt-error">{error}</p>}
        {result && <div className="support-results">
          <div className="support-baseline"><strong>{st('baseline')}</strong>
            <span>DPS {number(result.baseline.TotalDPS ?? 0)}</span><span>EHP {number(result.baseline.TotalEHP ?? 0)}</span>
          </div>
          <p className="skills-hint">{st('results')} {result.evaluated} · {st('unsupported')} {result.unmodeled}</p>
          {!result.plans.length && <p className="skills-hint">{st('noGain')}</p>}
          {result.plans.map((plan, index) => <article className="support-plan" key={index}>
            <div className="support-plan-gems">{plan.supports.map(gem => <span className="opt-chip" key={gem.skill_id} data-skill-id={gem.skill_id}>
              {gemName(gem.skill_id)}{byId.get(gem.skill_id)?.is_lineage && <span className="gem-lineage-badge">{tt('picker.lineage')}</span>}
              <button className="support-exclude-button" aria-label={`${st('exclude')} ${gemName(gem.skill_id)}`}
                title={`${st('exclude')} ${gemName(gem.skill_id)}`} onClick={() => exclude(gem.skill_id)}><span aria-hidden>×</span></button>
            </span>)}</div>
            <div className="support-plan-metrics">{['TotalDPS', 'TotalEHP'].map(stat => {
              const before = result.baseline[stat] ?? 0;
              const after = plan.stats[stat] ?? 0;
              return <span key={stat} className={after >= before ? 'is-positive' : 'is-negative'}>
                {stat === 'TotalDPS' ? 'DPS' : 'EHP'} {number(after)} <strong>{change(before, after)}</strong>
              </span>;
            })}</div>
            <button disabled={session.busy} onClick={() => apply(index)}>{tt('opt.apply')}</button>
          </article>)}
          <p className="skills-hint">{st('keepActive')}</p>
        </div>}
        <p className="skills-hint">{st('adjustment')}</p>
        <p className="skills-hint">{st('limit')}</p>
      </div>}
    </div>
  );
}
