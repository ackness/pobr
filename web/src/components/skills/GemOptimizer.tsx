import { useGemDescriptions } from '../../hooks/useGemDescriptions';
import { GemDescriptionPanel } from './GemDescriptionPanel';
import { DisclosureChevron } from '../shared/DisclosureChevron';
import { UnincludedModifiers } from '../shared/UnincludedModifiers';
import { useJobBeacon } from '../../app/jobs/useJobBeacon';
import { usePreview } from '../shared/PreviewContext';
import { readPreference, uiStorageKey, writePreference } from '../../lib/sessionStorage';
import { AppButton, AppInput } from '../shared/controls';
import { formatApiError } from '../../api/error';
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { CalculateBuildRequest, GemCatalogEntry, SocketGroupInput } from '../../api/types';
import { prefersReducedMotion } from '../../app/motion/tween';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useUpgradeGoal } from '../../hooks/useUpgradeGoal';
import { bindT, statNameLabel, type Lang } from '../../lib/i18n';
import { changesFromRecords } from '../../app/preview/fromRecords';
import { formatNumber, statMap, formatStat } from '../../lib/statDisplay';
import { supportText } from '../../lib/supportI18n';
import { resolveDamageObjective } from '../../lib/mainMinion';
import { useHeadlineDamage } from '../../hooks/useMainMinion';
import type { Objective } from '../../lib/optimize';
import { applySupportPlan, eligibleSupports, lineageAvailable, optimizeSupports, type SupportMetadata, type SupportOptimization } from '../../lib/supportOptimizer';
import { ObjectiveEditor, OptimizerProgress, objectiveOf } from '../shared/OptimizerControls';
import { colon } from '../../app/text';

interface Props {
  session: BuildSession;
  lang: Lang;
  active: boolean;
  groupIndex: number;
  catalog: SupportMetadata[];
  gemName: (skillId: string) => string;
  gemCatalog?: ReadonlyMap<string, GemCatalogEntry>;
  skillKey: string;
  focusNonce?: number;
  onFocusConsumed?: (nonce: number) => void;
}

interface SupportEvaluation extends SupportOptimization {
  request: CalculateBuildRequest;
  requestKey: string;
  settingsKey: string;
  objective: Objective;
  catalog: SupportMetadata[];
  dataVersion: BuildSession['dataVersion'];
}

interface AppliedSupports {
  scope: BuildSession['calculationScope'];
  version: number;
  annotations: BuildSession['annotations'];
  group: SocketGroupInput;
  skillId: string;
  added: string[];
  removed: string[];
  damageStat: string;
  before: Record<string, number>;
  after: Record<string, number>;
}

function savedExclusions(key: string): string[] {
  try {
    const value: unknown = JSON.parse(readPreference(key) ?? '[]');
    if (Array.isArray(value)) return [...new Set(value.filter((id): id is string => typeof id === 'string'))];
  } catch { /* Invalid or unavailable storage leaves every candidate enabled. */ }
  return [];
}

export function GemOptimizer({ session, lang, active, groupIndex, catalog, gemName, gemCatalog, skillKey, focusNonce, onFocusConsumed }: Props) {
  const preview = usePreview();
  const tt = bindT(lang);
  const st = (key: Parameters<typeof supportText>[1]) => supportText(lang, key);
  const { goal, setGoal } = useUpgradeGoal();
  const damage = useHeadlineDamage(session.calc, lang);
  const group = session.socketGroups[groupIndex];
  const byId = useMemo(() => new Map(catalog.map(gem => [gem.skill_id, gem])), [catalog]);
  const equipped = useMemo(() => group?.gems.filter(gem => byId.get(gem.skill_id)?.is_support) ?? [], [group, byId]);
  const occupied = equipped.length;
  const [expanded, setExpanded] = useState(false);
  const descriptions = useGemDescriptions(active && expanded);
  const [capacity, setCapacity] = useState(() => Math.max(2, Math.min(5, occupied)));
  const minCapacity = Math.min(5, occupied);
  if (capacity < minCapacity) setCapacity(minCapacity);
  const [includeLineage, setIncludeLineage] = useState(true);
  const preferenceKey = uiStorageKey(`pobr-support-exclusions:${skillKey}`);
  const [excludedIds, setExcludedIds] = useState(() => savedExclusions(preferenceKey));
  const excluded = useMemo(() => new Set(excludedIds), [excludedIds]);
  const [search, setSearch] = useState('');
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  useJobBeacon(progress !== null);
  const [error, setError] = useState<string | null>(null);
  const [evaluated, setEvaluated] = useState<SupportEvaluation | null>(null);
  const [receipt, setReceipt] = useState<AppliedSupports | null>(null);
  const abortRef = useRef<AbortController | null>(null);
  const settingsRef = useRef<HTMLDetailsElement | null>(null);
  const toggleRef = useRef<HTMLButtonElement>(null);
  const runRef = useRef<HTMLButtonElement>(null);
  const appliedRef = useRef<HTMLDivElement>(null);
  const focusedReceipt = useRef<AppliedSupports | null>(null);
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
  const requestKey = useMemo(() => JSON.stringify(session.currentRequest()), [session.currentRequest, session.stateVersion]);
  const settingsKey = useMemo(() => JSON.stringify([requestKey, session.stateVersion, session.socketGroupRevision,
    session.dataVersion, session.treeVersion, session.activeWeaponSet, groupIndex, capacity, includeLineage, goal, excludedIds, damage.stat]),
  [requestKey, session.stateVersion, session.socketGroupRevision, session.dataVersion, session.treeVersion,
    session.activeWeaponSet, groupIndex, capacity, includeLineage, goal, excludedIds, damage.stat]);
  const result = evaluated?.settingsKey === settingsKey && evaluated.requestKey === requestKey
    && evaluated.catalog === catalog ? evaluated : null;
  // The installed group and its edit revision own the receipt, not the obsolete evaluation.
  const applied = receipt && receipt.scope === session.calculationScope && receipt.version === session.stateVersion
    && receipt.group === group ? receipt : null;
  const latest = useRef({ active, result, applied, session });
  latest.current = { active, result, applied, session };
  const stop = () => {
    abortRef.current?.abort();
    abortRef.current = null;
    setProgress(null);
  };
  const invalidate = () => {
    stop();
    latest.current.result = null;
    setEvaluated(null);
    setError(null);
  };
  const updateExclusions = (next: string[]) => {
    // Invalidate before publishing new settings or allowing another Apply action.
    invalidate();
    setExcludedIds(next);
    writePreference(preferenceKey, JSON.stringify(next));
  };
  const exclude = (id: string) => updateExclusions([...new Set([...excludedIds, id])]);
  const restore = (id: string) => updateExclusions(excludedIds.filter(value => value !== id));
  const mainIndex = session.calcParams.main_socket_group;
  const select = () => {
    if (mainIndex !== groupIndex) session.updateParams({ main_socket_group: groupIndex });
  };
  useLayoutEffect(() => {
    if (active && focusNonce !== undefined) setExpanded(true);
  }, [active, focusNonce]);
  useEffect(() => {
    if (!active || !expanded || focusNonce === undefined) return;
    // The shell resets the page scroll on navigation. Focus after that reset,
    // and prevent the browser from starting a second scroll for focus itself.
    const frame = requestAnimationFrame(() => {
      const target = runRef.current?.disabled === false ? runRef.current : toggleRef.current;
      if (!target) return;
      onFocusConsumed?.(focusNonce);
      target.focus({ preventScroll: true });
      target.scrollIntoView({ block: 'center', behavior: prefersReducedMotion() ? 'auto' : 'smooth' });
    });
    return () => cancelAnimationFrame(frame);
  }, [active, expanded, focusNonce, onFocusConsumed]);
  useLayoutEffect(() => {
    const target = appliedRef.current;
    if (!active || !expanded || !applied || !target || focusedReceipt.current === applied) return;
    focusedReceipt.current = applied;
    target.focus({ preventScroll: true });
    target.scrollIntoView({ block: 'nearest', behavior: 'instant' });
  }, [active, expanded, applied]);
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
    const objective = resolveDamageObjective(objectiveOf(goal, Object.fromEntries([...statMap(session.calc?.stats ?? [])]
      .map(([key, value]) => [key, value ?? 0]))), session.calc);
    invalidate();
    setReceipt(null);
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
    const plan = result?.plans[index];
    const skillId = session.calc?.main_skill?.skill_id;
    if (!active || !latest.current.active || session !== latest.current.session || session.busy
      || progress !== null || !result || latest.current.result !== result || !plan || !skillId
      || result.requestKey !== JSON.stringify(session.currentRequest())) return;
    const groups = applySupportPlan(session.socketGroups, groupIndex, plan);
    const installedGroup = groups[groupIndex];
    if (!installedGroup || installedGroup === group) return;
    const { added, removed } = supportEdits(plan.supports);
    invalidate();
    session.setSocketGroups(groups);
    setReceipt({ scope: session.calculationScope, version: session.stateVersion + 1, annotations: session.annotations,
      group: installedGroup, skillId, added, removed, damageStat: damage.stat, before: result.baseline, after: plan.stats });
  };
  const undo = () => {
    const current = latest.current;
    // Session undo restores the whole state; annotations do not advance its calculation version.
    if (!applied || current.applied !== applied || !current.active || current.session.busy
      || !current.session.canUndo || current.session.annotations !== applied.annotations) return;
    current.applied = null;
    current.session.undoEdit();
    setReceipt(null);
    const target = toggleRef.current;
    target?.focus({ preventScroll: true });
    target?.scrollIntoView({ block: 'nearest', behavior: 'instant' });
  };
  const metricLabel = (stat: string) => statNameLabel(lang, stat);
  const change = (before: number, after: number, stat: string) => before > 0
    ? `${after >= before ? '+' : ''}${formatNumber((after / before - 1) * 100, 1, 1)}%` : `${after >= before ? '+' : ''}${formatStat(stat, after - before, lang)}`;
  const supportEdits = (supports: readonly { skill_id: string }[]) => ({
    added: supports.filter(gem => !equipped.some(old => old.skill_id === gem.skill_id)).map(gem => gem.skill_id),
    removed: equipped.filter(gem => !supports.some(next => next.skill_id === gem.skill_id)).map(gem => gem.skill_id),
  });
  const editNames = (ids: string[]) => ids.map(gemName).join(' · ') || st('none');
  const metrics = (before: Record<string, number>, after: Record<string, number>, damageStat: string, damageLabel?: string) =>
    <div className="support-plan-metrics">{[damageStat, 'TotalEHP'].map(stat => {
      const old = before[stat];
      const next = after[stat];
      const trend = old === undefined || next === undefined || next === old ? undefined : next > old ? 'is-positive' : 'is-negative';
      return <span key={stat} className={trend}>
        <span className="support-metric-label">{stat === damageStat && damageLabel ? damageLabel : metricLabel(stat)}</span>{' '}
        <b>{next === undefined ? '—' : formatStat(stat, next, lang)}</b>{' '}
        {old !== undefined && next !== undefined && <strong>{change(old, next, stat)}</strong>}
      </span>;
    })}</div>;

  if (!active) return null;

  const hint = <p className="skills-hint">{st('hint')}</p>;
  const poolSummary = <div className="support-pool-summary"><strong>{candidateCount}</strong> {st('eligible')}
    <span>{st('excluded').replace('{total}', String(pool.unknown + pool.levelBlocked + pool.incompatible))
      .replace('{unknown}', String(pool.unknown)).replace('{level}', String(pool.levelBlocked))
      .replace('{incompatible}', String(pool.incompatible))}</span>
  </div>;
  const poolFilter = <details className="support-pool-details"><summary className="disclosure">{st('filterTitle')}</summary>
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
  </details>;
  const excludedBlock = excludedIds.length > 0 && <div className="support-excluded">
    <div className="support-excluded-header"><strong>{st('manualExcluded')} · {excludedIds.length}</strong>
      <AppButton onClick={() => updateExclusions([])}>{st('restoreAll')}</AppButton></div>
    <div className="opt-candidates">{excludedIds.map(id => <AppButton key={id} className="opt-chip"
      aria-label={`${st('restore')} ${gemName(id)}`} title={`${st('restore')} ${gemName(id)}`} onClick={() => restore(id)}>
      {gemName(id)} <span aria-hidden>↶</span>
    </AppButton>)}</div>
    <p className="skills-hint">{st('excludedHint')}</p>
  </div>;
  const capacityControl = <label>{st('capacity')}<AppInput type="number" min={2} max={5} value={capacity} onChange={event => {
    const value = Number(event.target.value);
    if (Number.isInteger(value) && value >= 2 && value <= 5) {
      invalidate();
      setCapacity(value);
    }
  }} /></label>;
  const objectiveControls = <ObjectiveEditor damage={damage} value={goal} onChange={next => { invalidate(); setGoal(next); }} lang={lang} />;
  const runButton = <AppButton ref={runRef} className="opt-run" disabled={session.busy || progress !== null || !canSearch || !group?.enabled
    || (mainIndex === groupIndex && !session.calc?.main_skill)} onClick={run}>
    {progress ? tt('opt.running') : mainIndex !== groupIndex ? st('selectMain') : st('run')}
  </AppButton>;
  const lineageOption = <label className="support-lineage-option"><AppInput type="checkbox" checked={includeLineage}
    onChange={event => { invalidate(); setIncludeLineage(event.target.checked); }} />{st('lineage')}</label>;
  const capacityHint = <p className="skills-hint">{st('capacityHint')}</p>;
  const noMainSkill = !session.busy && mainIndex === groupIndex && !session.calc?.main_skill && <p className="skills-hint">{tt('sidebar.noMainSkill')}</p>;
  const progressBlock = progress && <OptimizerProgress {...progress} onCancel={stop} lang={lang} />;
  const poolNotes = <>
    {!pool.gems.length && <p className="skills-hint">{st('empty')}</p>}
    {pool.gems.length > 0 && !candidateCount && <p className="skills-hint">{st(occupied ? 'removeOnly' : 'allExcluded')}</p>}
  </>;
  const errorBlock = error && <p className="opt-error">{error}</p>;
  const resultsBlock = result && <div className="support-results">
    <div className="support-baseline"><strong>{st('baseline')}</strong>
      <span>{damage.label} {result.baseline[damage.stat] === undefined ? '—' : formatStat(damage.stat, result.baseline[damage.stat], lang)}</span><span>{metricLabel('TotalEHP')} {result.baseline.TotalEHP === undefined ? '—' : formatStat('TotalEHP', result.baseline.TotalEHP, lang)}</span>
    </div>
    {damage.note && <p className="skills-hint">{damage.note}</p>}
    <p className="skills-hint">{st('keepActive')}</p>
    <details className="support-calculation-details">
    <summary className="disclosure">{st('calculationDetails')}</summary>
    <p className="skills-hint">{st('results')} {result.evaluated} · {st('unsupported')} {result.unmodeled}</p>
    <p className="skills-hint">{st('data')}{colon(lang)}{result.dataVersion ?? st('unknownVersion')} · {st('tree')}{colon(lang)}{result.request.tree_version ?? st('defaultTree')}</p>
    {result.baselineUnsupported.length > 0 && <details className="support-diagnostics support-baseline-unsupported">
      <summary className="disclosure" data-disclosure-level="group">{st('baselineLimits')}{colon(lang)}{result.baselineUnsupported.length}</summary>
      <p className="skills-hint">{st('baselineLimitsHint')}</p>
      <UnincludedModifiers lines={result.baselineUnsupported} lang={lang} items={result.request.items} />
    </details>}
    {result.excluded.length > 0 && <details className="support-diagnostics">
      <summary className="disclosure" data-disclosure-level="group">{st('diagnosticExcluded')}{colon(lang)}{result.excluded.length}</summary>
      <p className="skills-hint">{st('diagnosticExcludedHint')}</p>
      <ol className="support-diagnostic-excluded">{result.excluded.map((edit, index) => <li key={index}>
        <details>
          <summary className="disclosure">{st('completeSet')}{colon(lang)}{edit.supports.map(gem => gemName(gem.skill_id)).join(' · ') || st('noSupports')}</summary>
          <ul className="support-diagnostic-gems">{edit.supports.map(gem => <li key={gem.skill_id} data-skill-id={gem.skill_id}>
            {gemName(gem.skill_id)} · {st('level')} {gem.level} · {st('quality')} {gem.quality} · {st('statSet')} {gem.stat_set_index ?? 1}
          </li>)}</ul>
          {edit.unsupported.length > 0 && <><p>{st('newLimits')}</p>
            <UnincludedModifiers lines={edit.unsupported} lang={lang} /></>}
          {edit.error && <p className="opt-error">{st('engineError')}{colon(lang)}{edit.error}</p>}
          {edit.invalidStats && <p className="opt-error">{st('invalidStats')}</p>}
        </details>
      </li>)}</ol>
    </details>}
    </details>
    {result.evaluated === 0 ? <div role="status">
      <p className="skills-hint">{st('notEvaluated')} {st(capacity < occupied ? 'blockedCapacity'
        : !candidateCount ? pool.unknown ? 'blockedUnknown' : 'blockedCandidates' : 'blockedCompatibility')}</p>
      <AppButton onClick={() => { if (settingsRef.current) { settingsRef.current.open = true; settingsRef.current.querySelector('input')?.focus(); } }}>{st('adjustSearch')}</AppButton>
    </div> : !result.plans.length && <p className="skills-hint">{st('noGain')}</p>}
    {result.plans.map((plan, index) => {
      const changes = changesFromRecords(
        Object.fromEntries([damage.stat, 'TotalEHP'].flatMap(id => result.baseline[id] === undefined ? [] : [[id, result.baseline[id]]])),
        Object.fromEntries([damage.stat, 'TotalEHP'].flatMap(id => plan.stats[id] === undefined ? [] : [[id, plan.stats[id]]])));
      const hover = preview ? {
        onEnter: (event: { currentTarget: Element }) => preview.showChanges(tt('opt.previewSupports', { supports: plan.supports.map(gem => gemName(gem.skill_id)).join(' + ') }), changes, event.currentTarget),
        onLeave: preview.clear } : undefined;
      const { added, removed } = supportEdits(plan.supports);
      return <article className="support-plan" key={index} onMouseEnter={hover?.onEnter} onMouseLeave={hover?.onLeave} onFocus={hover?.onEnter} onBlur={hover?.onLeave}>
        {metrics(result.baseline, plan.stats, damage.stat, damage.label)}
        <dl className="support-plan-edits">
          <dt>{st('equip')}</dt><dd>{editNames(added)}</dd>
          <dt>{st('remove')}</dt><dd>{editNames(removed)}</dd>
        </dl>
        <div className="support-plan-actions">
          <details className="support-plan-details">
            <summary className="disclosure">{st('completeSet')}</summary>
            {plan.supports.length ? <div className="support-plan-gems">{plan.supports.map(gem => <span className="opt-chip" key={gem.skill_id} data-skill-id={gem.skill_id}
              data-level={gem.level} data-quality={gem.quality} data-stat-set-index={gem.stat_set_index ?? undefined}>
              {gemName(gem.skill_id)}{byId.get(gem.skill_id)?.is_lineage && <span className="gem-lineage-badge">{tt('picker.lineage')}</span>}
              <small>{st('level')} {gem.level} · {st('quality')} {gem.quality}</small>
              <AppButton className="support-exclude-button" aria-label={`${st('exclude')} ${gemName(gem.skill_id)}`}
                title={`${st('exclude')} ${gemName(gem.skill_id)}`} onClick={() => exclude(gem.skill_id)}><span aria-hidden>×</span></AppButton>
            </span>)}</div> : <p className="skills-hint">{st('noSupports')}</p>}
            {descriptions && <details className="support-diagnostics support-descriptions">
              <summary className="disclosure">{st('descriptions')}</summary>
              <p className="skills-hint">{tt('picker.descriptionHint')}</p>
              {plan.supports.map(gem => {
                const metadata = byId.get(gem.skill_id);
                const entry = gemCatalog?.get(gem.skill_id) ?? {
                  skill_id: gem.skill_id, name: metadata?.name ?? gemName(gem.skill_id),
                  name_zh_cn: null, name_zh_tw: null, is_support: true, is_lineage: metadata?.is_lineage ?? false, is_unique_only: false,
                  active_effect_ids: [], colour: null, tags: metadata?.require_skill_types ?? [],
                };
                return <GemDescriptionPanel key={gem.skill_id} gem={entry} index={descriptions} lang={lang} />;
              })}
            </details>}
          </details>
          <AppButton disabled={session.busy} onClick={() => apply(index)}>{st('apply')}</AppButton>
        </div>
      </article>;
    })}
  </div>;
  const toggleButton = <AppButton ref={toggleRef} className="gem-optimizer-toggle disclosure" data-disclosure-level="section" aria-expanded={expanded} onClick={() => {
    setExpanded(!expanded);
    if (!expanded) select();
  }}>
    <DisclosureChevron className="row-caret" />{st('title')}
  </AppButton>;

  // Manual picking is outside this optional section; calculated results precede settings.
  return (
    <div className="gem-optimizer">
      <div className="gem-optimizer-head">
        {toggleButton}
        {expanded && runButton}
      </div>
      {expanded && <div className="gem-optimizer-body">
        {lineageOption}
        {noMainSkill}
        {progressBlock}
        {poolNotes}
        {errorBlock}
        {resultsBlock}
        {applied && <div className="support-applied">
          <div role="status" tabIndex={-1} ref={appliedRef}>
            <p><strong>{st('applied').replace('{skill}', () => gemName(applied.skillId))}</strong></p>
            <dl className="support-plan-edits">
              {applied.added.length > 0 && <><dt>{st('added')}</dt><dd>{editNames(applied.added)}</dd></>}
              {applied.removed.length > 0 && <><dt>{st('removed')}</dt><dd>{editNames(applied.removed)}</dd></>}
            </dl>
            {metrics(applied.before, applied.after, applied.damageStat)}
          </div>
          {session.canUndo && applied.annotations === session.annotations
            && <AppButton disabled={session.busy} onClick={undo}>{tt('shell.undo')}</AppButton>}
        </div>}
        <details className="gem-optimizer-settings" ref={settingsRef}>
          <summary className="disclosure" data-disclosure-level="group">{tt('opt.candidateSettings')}</summary>
          <div className="gem-optimizer-settings-body">
            {hint}
            {poolSummary}
            <div className="opt-row">{capacityControl}{objectiveControls}</div>
            {capacityHint}
            {poolFilter}
            {excludedBlock}
            <p className="skills-hint">{st('adjustment')}</p>
            <p className="skills-hint">{st('limit')}</p>
          </div>
        </details>
      </div>}
    </div>
  );
}
