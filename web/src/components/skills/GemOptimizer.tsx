import { AppButton, AppInput } from '../shared/controls';
import { formatApiError } from '../../api/error';
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { CalculateBuildRequest } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useUpgradeGoal } from '../../hooks/useUpgradeGoal';
import { bindT, type Lang } from '../../lib/i18n';
import { statMap } from '../../lib/statDisplay';
import { supportText } from '../../lib/supportI18n';
import type { Objective } from '../../lib/optimize';
import { applySupportPlan, eligibleSupports, lineageAvailable, optimizeSupports, type SupportMetadata, type SupportOptimization } from '../../lib/supportOptimizer';
import { ObjectiveEditor, OptimizerProgress, objectiveOf } from '../shared/OptimizerControls';

interface Props {
  session: BuildSession;
  lang: Lang;
  active: boolean;
  groupIndex: number;
  catalog: SupportMetadata[];
  gemName: (skillId: string) => string;
  skillKey: string;
  focusNonce?: number;
}

interface SupportEvaluation extends SupportOptimization {
  request: CalculateBuildRequest;
  requestKey: string;
  settingsKey: string;
  objective: Objective;
  catalog: SupportMetadata[];
  dataVersion: BuildSession['dataVersion'];
}

function savedExclusions(key: string): string[] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(key) ?? '[]');
    if (Array.isArray(value)) return [...new Set(value.filter((id): id is string => typeof id === 'string'))];
  } catch { /* Invalid or unavailable storage leaves every candidate enabled. */ }
  return [];
}

export function GemOptimizer({ session, lang, active, groupIndex, catalog, gemName, skillKey, focusNonce }: Props) {
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
  const [evaluated, setEvaluated] = useState<SupportEvaluation | null>(null);
  const abortRef = useRef<AbortController | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const consumedFocus = useRef<number | undefined>(undefined);
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
  const requestKey = JSON.stringify(session.currentRequest());
  const settingsKey = JSON.stringify([requestKey, session.stateVersion, session.socketGroupRevision,
    session.dataVersion, session.treeVersion, session.activeWeaponSet, groupIndex, capacity, includeLineage, goal, excludedIds]);
  const result = evaluated?.settingsKey === settingsKey && evaluated.requestKey === requestKey
    && evaluated.catalog === catalog ? evaluated : null;
  const stop = () => {
    abortRef.current?.abort();
    abortRef.current = null;
    setProgress(null);
  };
  const invalidate = () => {
    stop();
    setEvaluated(null);
    setError(null);
  };
  const updateExclusions = (next: string[]) => {
    // Invalidate before publishing new settings or allowing another Apply action.
    invalidate();
    setExcludedIds(next);
    try { localStorage.setItem(preferenceKey, JSON.stringify(next)); } catch { /* In-memory filtering still works. */ }
  };
  const exclude = (id: string) => updateExclusions([...new Set([...excludedIds, id])]);
  const restore = (id: string) => updateExclusions(excludedIds.filter(value => value !== id));
  const mainIndex = session.calcParams.main_socket_group;
  const select = () => {
    if (mainIndex !== groupIndex) session.updateParams({ main_socket_group: groupIndex });
  };
  useLayoutEffect(() => {
    if (!active || focusNonce === undefined || consumedFocus.current === focusNonce) return;
    consumedFocus.current = focusNonce;
    setExpanded(true);
    rootRef.current?.scrollIntoView({ block: 'center', behavior: 'smooth' });
  }, [active, focusNonce]);
  useLayoutEffect(() => {
    invalidate();
  }, [settingsKey, catalog]);
  useLayoutEffect(() => {
    if (!active) stop();
  }, [active]);
  useLayoutEffect(() => () => {
    abortRef.current?.abort();
    abortRef.current = null;
  }, []);

  const run = async () => {
    if (!active || session.busy || progress !== null || !group?.enabled) return;
    if (mainIndex !== groupIndex) { select(); return; }
    const request = session.currentRequest();
    if (!request || !canSearch || !session.calc?.main_skill || session.calcError) return;
    const objective = objectiveOf(goal, Object.fromEntries([...statMap(session.calc?.stats ?? [])]
      .map(([key, value]) => [key, value ?? 0])));
    invalidate();
    const controller = new AbortController();
    abortRef.current = controller;
    const requestKey = JSON.stringify(request);
    setProgress({ done: 0, total: candidateCount });
    try {
      const result = await optimizeSupports({ request, groupIndex, catalog, capacity, includeLineage,
        excludedSkillIds: excludedIds, objective,
        signal: controller.signal, onProgress: (done, total) => {
          if (!controller.signal.aborted && abortRef.current === controller) setProgress({ done, total });
        } });
      if (!controller.signal.aborted && abortRef.current === controller) {
        setEvaluated({ ...result, request, requestKey, settingsKey, objective, catalog, dataVersion: session.dataVersion });
      }
    } catch (err: unknown) {
      if (!controller.signal.aborted && abortRef.current === controller) setError(formatApiError(err));
    } finally {
      if (abortRef.current === controller) { setProgress(null); abortRef.current = null; }
    }
  };
  const apply = (index: number) => {
    if (!active || session.busy || progress !== null || !result || !result.plans[index]
      || result.requestKey !== JSON.stringify(session.currentRequest())) return;
    const groups = applySupportPlan(session.socketGroups, groupIndex, result.plans[index]);
    invalidate();
    session.setSocketGroups(groups);
  };
  const number = (value: number) => value.toLocaleString(lang, { maximumFractionDigits: 1 });
  const change = (before: number, after: number) => before > 0
    ? `${after >= before ? '+' : ''}${((after / before - 1) * 100).toFixed(1)}%` : `${after >= before ? '+' : ''}${number(after - before)}`;

  if (!active) return null;

  return (
    <div className="gem-optimizer" ref={rootRef}>
      <AppButton className="gem-optimizer-toggle" aria-expanded={expanded} onClick={() => {
        setExpanded(!expanded);
        if (!expanded) select();
      }}>
        <span className="row-caret" aria-hidden>▸</span>{st('title')}
      </AppButton>
      {expanded && <div className="gem-optimizer-body">
        <p className="skills-hint">{st('hint')}</p>
        <div className="support-pool-summary"><strong>{candidateCount}</strong> {st('eligible')}
          <span>{st('excluded')} {pool.unknown} / {pool.levelBlocked} / {pool.incompatible}</span>
        </div>
        <details className="support-pool-details"><summary>{st('filterTitle')}</summary>
          <p className="skills-hint">{st('filterHint')}</p>
          <label className="support-search">{st('search')}<AppInput type="search" value={search}
            placeholder={st('searchPlaceholder')} onChange={event => setSearch(event.target.value)} /></label>
          <div className="opt-candidates">{visibleGems.map(gem => <label key={gem.skill_id}
            className={`opt-chip support-candidate${excluded.has(gem.skill_id) ? ' is-excluded' : ''}`} data-skill-id={gem.skill_id}>
            <AppInput type="checkbox" checked={!excluded.has(gem.skill_id)}
              onChange={event => event.target.checked ? restore(gem.skill_id) : exclude(gem.skill_id)} />
            <span>{gemName(gem.skill_id)}</span>{gem.is_lineage && <span className="gem-lineage-badge">{tt('picker.lineage')}</span>}
          </label>)}</div>
          {!visibleGems.length && <p className="skills-hint">{st('noMatch')}</p>}
        </details>
        {excludedIds.length > 0 && <div className="support-excluded">
          <div className="support-excluded-header"><strong>{st('manualExcluded')} · {excludedIds.length}</strong>
            <AppButton onClick={() => updateExclusions([])}>{st('restoreAll')}</AppButton></div>
          <div className="opt-candidates">{excludedIds.map(id => <AppButton key={id} className="opt-chip"
            aria-label={`${st('restore')} ${gemName(id)}`} title={`${st('restore')} ${gemName(id)}`} onClick={() => restore(id)}>
            {gemName(id)} <span aria-hidden>↶</span>
          </AppButton>)}</div>
          <p className="skills-hint">{st('excludedHint')}</p>
        </div>}
        <div className="opt-row">
          <label>{st('capacity')}<AppInput type="number" min={2} max={5} value={capacity} onChange={event => {
            const value = Number(event.target.value);
            if (Number.isInteger(value) && value >= 2 && value <= 5) {
              invalidate();
              setCapacity(value);
            }
          }} /></label>
          <ObjectiveEditor value={goal} onChange={next => { invalidate(); setGoal(next); }} lang={lang} />
          <AppButton className="opt-run" disabled={session.busy || progress !== null || !canSearch || !group?.enabled
            || (mainIndex === groupIndex && !session.calc?.main_skill)} onClick={run}>
            {progress ? tt('opt.running') : mainIndex !== groupIndex ? st('selectMain') : st('run')}
          </AppButton>
        </div>
        <label className="support-lineage-option"><AppInput type="checkbox" checked={includeLineage}
          onChange={event => { invalidate(); setIncludeLineage(event.target.checked); }} />{st('lineage')}</label>
        <p className="skills-hint">{st('capacityHint')}</p>
        {!session.busy && mainIndex === groupIndex && !session.calc?.main_skill && <p className="skills-hint">{tt('sidebar.noMainSkill')}</p>}
        {progress && <OptimizerProgress {...progress} onCancel={stop} lang={lang} />}
        {!pool.gems.length && <p className="skills-hint">{st('empty')}</p>}
        {pool.gems.length > 0 && !candidateCount && <p className="skills-hint">{st(occupied ? 'removeOnly' : 'allExcluded')}</p>}
        {error && <p className="opt-error">{error}</p>}
        {result && <div className="support-results">
          <div className="support-baseline"><strong>{st('baseline')}</strong>
            <span>DPS {number(result.baseline.TotalDPS ?? 0)}</span><span>EHP {number(result.baseline.TotalEHP ?? 0)}</span>
          </div>
          <p className="skills-hint">{st('results')} {result.evaluated} · {st('unsupported')} {result.unmodeled}</p>
          <p className="skills-hint">{st('data')}: {result.dataVersion ?? st('unknownVersion')} · {st('tree')}: {result.request.tree_version ?? st('defaultTree')}</p>
          {result.baselineUnsupported.length > 0 && <details className="support-diagnostics support-baseline-unsupported">
            <summary>{st('baselineLimits')}: {result.baselineUnsupported.length}</summary>
            <p className="skills-hint">{st('baselineLimitsHint')}</p>
            <ul>{result.baselineUnsupported.map((reason, index) => <li key={index}>{reason}</li>)}</ul>
          </details>}
          {result.excluded.length > 0 && <details className="support-diagnostics">
            <summary>{st('diagnosticExcluded')}: {result.excluded.length}</summary>
            <p className="skills-hint">{st('diagnosticExcludedHint')}</p>
            <ol className="support-diagnostic-excluded">{result.excluded.map((edit, index) => <li key={index}>
              <details>
                <summary>{st('completeSet')}: {edit.supports.map(gem => gemName(gem.skill_id)).join(' · ') || st('noSupports')}</summary>
                <ul className="support-diagnostic-gems">{edit.supports.map(gem => <li key={gem.skill_id} data-skill-id={gem.skill_id}>
                  {gemName(gem.skill_id)} · {st('level')} {gem.level} · {st('quality')} {gem.quality} · {st('statSet')} {gem.stat_set_index ?? 1}
                </li>)}</ul>
                {edit.unsupported.length > 0 && <><p>{st('newLimits')}</p>
                  <ul>{edit.unsupported.map((reason, reasonIndex) => <li key={reasonIndex}>{reason}</li>)}</ul></>}
                {edit.error && <p className="opt-error">{st('engineError')}: {edit.error}</p>}
                {edit.invalidStats && <p className="opt-error">{st('invalidStats')}</p>}
              </details>
            </li>)}</ol>
          </details>}
          {!result.plans.length && <p className="skills-hint">{st('noGain')}</p>}
          {result.plans.map((plan, index) => <article className="support-plan" key={index}>
            <div className="support-plan-gems">{plan.supports.map(gem => <span className="opt-chip" key={gem.skill_id} data-skill-id={gem.skill_id}
              data-level={gem.level} data-quality={gem.quality} data-stat-set-index={gem.stat_set_index ?? undefined}>
              {gemName(gem.skill_id)}{byId.get(gem.skill_id)?.is_lineage && <span className="gem-lineage-badge">{tt('picker.lineage')}</span>}
              <small>{st('level')} {gem.level} · {st('quality')} {gem.quality}{gem.stat_set_index != null && <> · {st('statSet')} {gem.stat_set_index}</>}</small>
              <AppButton className="support-exclude-button" aria-label={`${st('exclude')} ${gemName(gem.skill_id)}`}
                title={`${st('exclude')} ${gemName(gem.skill_id)}`} onClick={() => exclude(gem.skill_id)}><span aria-hidden>×</span></AppButton>
            </span>)}</div>
            <div className="support-plan-metrics">{['TotalDPS', 'TotalEHP'].map(stat => {
              const before = result.baseline[stat] ?? 0;
              const after = plan.stats[stat] ?? 0;
              return <span key={stat} className={after >= before ? 'is-positive' : 'is-negative'}>
                {stat === 'TotalDPS' ? 'DPS' : 'EHP'} {number(after)} <strong>{change(before, after)}</strong>
              </span>;
            })}</div>
            {plan.supports.length > 0 && <details className="support-diagnostics support-descriptions">
              <summary>{st('descriptions')}</summary>
              <p className="skills-hint">{st('descriptionsHint')}</p>
              <ul>{plan.supports.map(gem => {
                const description = byId.get(gem.skill_id)?.description;
                return <li key={gem.skill_id}>
                  <strong>{gemName(gem.skill_id)}</strong>
                  <p lang={description ? 'en' : lang}>{description || st('descriptionUnavailable')}</p>
                </li>;
              })}</ul>
            </details>}
            <AppButton disabled={session.busy} onClick={() => apply(index)}>{tt('opt.apply')}</AppButton>
          </article>)}
          <p className="skills-hint">{st('keepActive')}</p>
        </div>}
        <p className="skills-hint">{st('adjustment')}</p>
        <p className="skills-hint">{st('limit')}</p>
      </div>}
    </div>
  );
}
