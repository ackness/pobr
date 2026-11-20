import { colon } from '../../app/text';
import { DisclosureChevron } from '../shared/DisclosureChevron';
import { formatNumber, formatStat } from '../../lib/statDisplay';
import { UnincludedModifiers } from '../shared/UnincludedModifiers';
import { defaultTravelAttribute, passivePlanOverflow, treeStatLabel, type passivePointSummary, type PassivePlannerMode } from './treePresentation';
import { useAppDialogs } from '../shared/AppDialogs';
import { useJobBeacon } from '../../app/jobs/useJobBeacon';
import { AppButton, AppInput } from '../shared/controls';
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type RefObject } from 'react';
import { formatApiError } from '../../api/error';
import type { AttributeChoice, CalculateBuildRequest, PassiveNode } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, messages, statNameLabel, type Lang } from '../../lib/i18n';
import { resolveDamageObjective } from '../../lib/mainMinion';
import { useHeadlineDamage } from '../../hooks/useMainMinion';
import { scoreOf, type Objective } from '../../lib/optimize';
import { useUpgradeGoal } from '../../hooks/useUpgradeGoal';
import {
  PASSIVE_PLAN_LIMIT,
  PASSIVE_POINT_LIMIT,
  passivePlanAllocation,
  passivePlanningContext,
  planPassiveUpgrades,
  withTravelAttributes,
  type PassivePlan,
  type PassivePlannerResult,
  type PassivePlanningContext,
} from '../../lib/passivePlanner';
import { AppSelect } from '../shared/AppSelect';
import { ObjectiveEditor, OptimizerProgress, objectiveOf, type ObjectiveState } from '../shared/OptimizerControls';
import { PlannerResultsNotice } from './PlannerResultsNotice';
import { WeaponSetControl } from '../shared/WeaponSetControl';
import { JewelPassivePlanner, useJewelPassivePlanner, type JewelPassivePlannerState } from './JewelPassivePlanner';

interface Props {
  session: BuildSession;
  lang: Lang;
  active: boolean;
  nodes: PassiveNode[];
  pointSummary: ReturnType<typeof passivePointSummary>;
  nodeLabel: (skill: number) => string;
  onLocate?: (skill: number) => void;
  onPreview: (plan: PassivePlan | null) => void;
  /** A search starts: whatever an earlier calculation painted on the tree is out of date. */
  onRun?: () => void;
  focusPlanner?: { nonce: number };
}

interface PassiveReceipt {
  key: string;
  request: CalculateBuildRequest;
  preparedRequest: CalculateBuildRequest;
  result: PassivePlannerResult;
  objective: Objective;
  dataVersion: BuildSession['dataVersion'];
  treeVersion: BuildSession['treeVersion'];
}

export interface TreeOptimizerState {
  session: BuildSession;
  lang: Lang;
  nodeLabel: (skill: number) => string;
  containerRef: RefObject<HTMLDivElement | null>;
  expanded: boolean;
  setExpanded: (value: boolean) => void;
  mode: PassivePlannerMode;
  setMode: (value: PassivePlannerMode) => void;
  points: number;
  setPoints: (value: number) => void;
  pointLimit: number;
  pointSummary: Props['pointSummary'];
  remaining: number | null;
  canReallocate: boolean;
  searchAllowed: boolean;
  goal: ObjectiveState;
  setGoal: (value: ObjectiveState) => void;
  attribute: AttributeChoice;
  setAttribute: (value: AttributeChoice) => void;
  context: PassivePlanningContext;
  tracking: boolean;
  progress: { done: number; total: number } | null;
  error: string | null;
  completed: PassiveReceipt | null;
  evaluated: PassivePlannerResult | null;
  preview: number | null;
  applied: PassivePlan | null;
  run: () => void;
  cancel: () => void;
  show: (index: number | null) => void;
  /** Drops the route either planner shows on the tree. */
  clearPreview: () => void;
  apply: (plan: PassivePlan) => Promise<void>;
  jewelPlanner: JewelPassivePlannerState;
}


export function useTreeOptimizer({ session, lang, active, nodes, pointSummary, nodeLabel, onLocate, onPreview, onRun, focusPlanner }: Props): TreeOptimizerState {
  const [expanded, setExpanded] = useState(Boolean(focusPlanner));
  const [modeOverride, setMode] = useState<PassivePlannerMode | null>(null);
  const [requestedPoints, setPoints] = useState(3);
  const dialogs = useAppDialogs(lang);
  const { goal, setGoal } = useUpgradeGoal();
  const [attributeOverride, setAttribute] = useState<AttributeChoice | null>(null);
  const attribute = useMemo(() => attributeOverride ?? defaultTravelAttribute(session.attributeChoices,
    session.treeMeta?.classes.find(entry => entry.name === session.character?.class_name)),
  [attributeOverride, session.attributeChoices, session.treeMeta, session.character?.class_name]);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  useJobBeacon(progress !== null);
  const [error, setError] = useState<string | null>(null);
  const [completed, setCompleted] = useState<PassiveReceipt | null>(null);
  const [applied, setApplied] = useState<{ version: number; plan: PassivePlan } | null>(null);
  const [tracking, setTracking] = useState(false);
  const [runId, setRunId] = useState(0);
  const [preview, setPreview] = useState<number | null>(null);
  const abortRef = useRef<AbortController | null>(null);
  const timerRef = useRef<number | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const consumedFocus = useRef<number | undefined>(undefined);
  const previewRef = useRef(onPreview);
  previewRef.current = onPreview;
  const context = useMemo(() => passivePlanningContext(nodes, session.allocatedNodes,
    session.character?.class_name, session.weaponSwap?.exclusive_nodes.flat() ?? [],
    session.jewels.map(jewel => jewel.socket_node), session.treeMeta?.classes.flatMap(entry => entry.ascendancies ?? [])
      .find(entry => entry.name === session.character?.ascendancy_name)?.id ?? session.character?.ascendancy_name,
    session.calc?.tree_effects),
  [nodes, session.allocatedNodes, session.character?.class_name, session.weaponSwap, session.jewels, session.treeMeta, session.character?.ascendancy_name, session.calc?.tree_effects]);
  const { max, used } = pointSummary;
  const remaining = used === null || max === null ? null : max - used;
  const canReallocate = context.canRefund && [...context.allocated].some(id => !context.protectedNodes.has(id));
  const mode = modeOverride ?? (remaining !== null && remaining <= 0 && canReallocate ? 'reallocate' : 'allocate');
  const searchMode = mode === 'future' ? 'allocate' : mode;
  const pointLimit = mode === 'allocate' ? Math.max(0, Math.min(PASSIVE_POINT_LIMIT, remaining ?? 0)) : PASSIVE_POINT_LIMIT;
  const points = Math.min(requestedPoints, pointLimit);
  const searchAllowed = remaining !== null && points > 0 && (mode !== 'reallocate' || context.canRefund);
  const baselineStats = useMemo(() => Object.fromEntries((session.calc?.stats ?? []).map(stat => [stat.id, stat.value ?? 0])), [session.calc]);
  const objective = useMemo(() => resolveDamageObjective(objectiveOf(goal, baselineStats), session.calc), [goal, baselineStats, session.calc]);
  const request = useMemo(() => session.currentRequest(), [session.currentRequest, session.stateVersion]);
  // Pair the full evaluation with its effective goal and authoritative jewel topology.
  const snapshotKey = useMemo(() => JSON.stringify([session.stateVersion, session.dataVersion, session.treeVersion,
    session.activeWeaponSet, request, session.calc?.tree_effects, mode, points, used, max, goal, attribute, objective]),
  [session.stateVersion, session.dataVersion, session.treeVersion, session.activeWeaponSet, request,
    session.calc?.tree_effects, mode, points, used, max, goal, attribute, objective]);
  const latestKeyRef = useRef(snapshotKey);
  latestKeyRef.current = snapshotKey;
  const activeRef = useRef(active);
  activeRef.current = active;
  const evaluated = completed?.key === snapshotKey ? completed.result : null;
  // A pending confirmation must not outlive the actionable view of this receipt.
  const readyReceipt = useRef<{ receipt: PassiveReceipt } | null>(null);
  useLayoutEffect(() => {
    readyReceipt.current = active && expanded && !session.busy && !session.calcError && evaluated && completed
      ? { receipt: completed } : null;
    return () => { readyReceipt.current = null; };
  }, [active, expanded, session.busy, session.calcError, completed, evaluated]);

  useLayoutEffect(() => {
    if (!active || !focusPlanner || !containerRef.current || consumedFocus.current === focusPlanner.nonce) return;
    consumedFocus.current = focusPlanner.nonce;
    setExpanded(true);
    containerRef.current?.scrollIntoView({ block: 'start' });
  }, [active, focusPlanner?.nonce, nodes]);

  const stop = () => {
    abortRef.current?.abort();
    abortRef.current = null;
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = null;
    setProgress(null);
    setPreview(null);
    previewRef.current(null);
  };
  const cancel = () => {
    stop();
    setTracking(false);
  };
  useLayoutEffect(() => {
    stop();
    setCompleted(null);
    setError(null);
  }, [snapshotKey]);
  useLayoutEffect(() => {
    if (!active) { cancel(); setError(null); }
  }, [active]);
  useLayoutEffect(() => () => {
    abortRef.current?.abort();
    abortRef.current = null;
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = null;
  }, []);

  useEffect(() => {
    abortRef.current?.abort();
    abortRef.current = null;
    setProgress(null);
    if (!active || !tracking || !expanded || completed?.key === snapshotKey
      || session.busy || session.calcError || !session.calc || !session.character
      || !request || context.root === null || !searchAllowed) return;
    const controller = new AbortController();
    abortRef.current = controller;
    // Coalesce rapid edits and wait for the session's new baseline before spending the search budget.
    const timer = window.setTimeout(async () => {
      if (timerRef.current === timer) timerRef.current = null;
      if (!activeRef.current || controller.signal.aborted || abortRef.current !== controller || latestKeyRef.current !== snapshotKey) return;
      const prepared = withTravelAttributes(request, nodes, attribute);
      setProgress({ done: 0, total: PASSIVE_PLAN_LIMIT });
      setCompleted(null);
      setError(null);
      try {
        const result = await planPassiveUpgrades({ request: prepared, context, points, mode: searchMode, objective,
          signal: controller.signal, onProgress: (done, total) => {
            if (activeRef.current && !controller.signal.aborted && abortRef.current === controller && latestKeyRef.current === snapshotKey) setProgress({ done, total });
          } });
        if (activeRef.current && !controller.signal.aborted && abortRef.current === controller && latestKeyRef.current === snapshotKey) {
          setCompleted({ key: snapshotKey, request, preparedRequest: prepared, result, objective,
            dataVersion: session.dataVersion, treeVersion: session.treeVersion });
        }
      } catch (err) {
        if (activeRef.current && !controller.signal.aborted && abortRef.current === controller && latestKeyRef.current === snapshotKey) setError(formatApiError(err));
      } finally {
        if (abortRef.current === controller) { setProgress(null); abortRef.current = null; }
      }
    }, 350);
    timerRef.current = timer;
    return () => {
      window.clearTimeout(timer);
      if (timerRef.current === timer) timerRef.current = null;
      controller.abort();
      if (abortRef.current === controller) abortRef.current = null;
    };
  }, [active, snapshotKey, tracking, expanded, completed, session.busy, session.calcError, session.calc, session.character, request,
    context, nodes, attribute, points, searchMode, searchAllowed, objective, runId]);

  const run = () => {
    if (!active || session.busy || session.calcError || !session.calc || !searchAllowed) return;
    stop();
    jewelPlanner.stop();
    onRun?.();
    setCompleted(null);
    setError(null);
    setTracking(true);
    setRunId(value => value + 1);
  };
  const show = (index: number | null) => {
    if (!active || latestKeyRef.current !== snapshotKey || (index !== null && !evaluated?.plans[index])) return;
    if (index !== null) jewelPlanner.clearPreview();
    setPreview(index);
    previewRef.current(index === null ? null : evaluated?.plans[index] ?? null);
  };
  const clearPreview = () => {
    setPreview(null);
    previewRef.current(null);
    jewelPlanner.clearPreview();
  };
  const apply = async (plan: PassivePlan) => {
    const ready = readyReceipt.current;
    if (!completed || completed !== ready?.receipt || completed.key !== latestKeyRef.current
      || !searchAllowed || remaining === null
      || JSON.stringify(session.currentRequest()) !== JSON.stringify(completed.request)
      || !completed.result.plans.some(candidate => candidate === plan)) return;
    const next = passivePlanAllocation(completed.preparedRequest, context, plan, points, searchMode);
    if (!next) return;
    const overflow = passivePlanOverflow(plan, remaining);
    if (mode === 'future' && overflow > 0) {
      const text = messages(lang, 'treeOptimizer');
      if (!await dialogs.confirm(text.confirmFuture.replace('{count}', String(overflow)), text.applyOverBudget)
        || ready !== readyReceipt.current || completed.key !== latestKeyRef.current) return;
    }
    // Apply the same full allocation and attribute choices used by the successful evaluation.
    readyReceipt.current = null;
    stop();
    setCompleted(null);
    session.setAllocatedNodes(next.allocatedNodes, next.attributeChoices);
    // A successful atomic session edit increments stateVersion exactly once.
    // Later edits cannot revive this receipt, even if they restore the same allocation.
    setApplied({ version: session.stateVersion + 1, plan });
  };
  const jewelPlanner = useJewelPassivePlanner({ session, lang, nodes, nodeLabel, onLocate, active: active && expanded,
    mode, points, remaining, searchAllowed, attribute, objective, onPreview: plan => {
      if (plan) setPreview(null);
      onPreview(plan);
    }, onStart: () => { cancel(); onRun?.(); } });
  return { session, lang, nodeLabel, containerRef, expanded, setExpanded, mode, setMode, points, setPoints,
    pointLimit, pointSummary, remaining, canReallocate, searchAllowed, goal, setGoal, attribute, setAttribute,
    context, tracking, progress, error, completed, evaluated, preview,
    applied: applied?.version === session.stateVersion && !session.busy && !session.calcError ? applied.plan : null,
    run, cancel, show, clearPreview, apply, jewelPlanner };
}

export function TreeOptimizer({ planner }: { planner: TreeOptimizerState }) {
  const { session, lang, nodeLabel, containerRef, expanded, setExpanded, mode, setMode, points, setPoints,
    pointLimit, pointSummary, remaining, canReallocate, searchAllowed, goal, setGoal, attribute, setAttribute,
    context, tracking, progress, error, completed, evaluated, preview, applied, run, cancel, show, apply, jewelPlanner } = planner;
  const text = messages(lang, 'treeOptimizer');
  const separator = colon(lang);
  const tt = bindT(lang);
  const number = (value: number) => formatNumber(value);
  const delta = (value: number, baseline: number) => baseline > 0 && Math.abs(value / baseline - 1) >= 0.0005
    ? `${value >= baseline ? '+' : ''}${formatNumber((value / baseline - 1) * 100, 1, 1)}%`
    : `${value >= baseline ? '+' : ''}${number(value - baseline)}`;
  const currentStats = evaluated?.baseline ?? {};
  const damage = useHeadlineDamage(session.calc, lang);
  const objective = completed?.objective;
  const metrics = [...new Set([objective?.stat ?? damage.stat, damage.stat, 'TotalEHP', 'Life'])];

  return (
    <div className="tree-planner" ref={containerRef} id="passive-upgrades">
      <AppButton className="tree-planner-toggle disclosure" data-disclosure-level="section" aria-expanded={expanded} onClick={() => { if (expanded) cancel(); setExpanded(!expanded); }}>
        <DisclosureChevron />{text.title}
      </AppButton>
      {expanded && <div className="tree-planner-body">
        {applied && <p className="tree-planner-hint tree-planner-applied" role="status">
          <strong>{text.applied}</strong><br />
          {text.add} {applied.allocate.length}{separator}{applied.allocate.map(nodeLabel).join(' · ') || '—'}<br />
          {text.remove} {applied.deallocate.length}{separator}{applied.deallocate.map(nodeLabel).join(' · ') || '—'}
        </p>}
        <WeaponSetControl session={session} lang={lang} />
        {session.character && <p className="tree-planner-hint tree-planner-budget" role="status">
          {remaining === null ? tt('tree.pointBudgetUnavailable') : <>
            {tt('tree.usedPoints', { used: pointSummary.used!, max: pointSummary.max! })}
            {' · '}{remaining > 0 ? text.budgetRemaining.replace('{count}', String(remaining))
              : remaining < 0 ? tt('tree.pointsOverBudget', { count: -remaining }) : text.budgetFull}
          </>}
        </p>}
        <div className="tree-planner-controls">
          <label>{text.mode}<AppSelect value={mode} ariaLabel={text.mode} onChange={value => setMode(value as PassivePlannerMode)} options={[
            { value: 'allocate', label: text.allocate }, { value: 'reallocate', label: text.reallocate }, { value: 'future', label: text.future },
          ]} /></label>
          <label>{mode === 'reallocate' ? text.refundPoints : text.points}<AppInput type="number" min={pointLimit > 0 ? 1 : 0} max={pointLimit} value={points} disabled={pointLimit === 0}
            onChange={event => { const value = Number(event.target.value); if (Number.isInteger(value) && value >= 1 && value <= pointLimit) setPoints(value); }} /></label>
          <ObjectiveEditor damage={damage} statName={id => treeStatLabel(lang, id)} baseline={Object.fromEntries((session.calc?.stats ?? []).filter(stat => stat.value !== null).map(stat => [stat.id, stat.value!]))} value={goal} onChange={setGoal} lang={lang} />
          <label>{text.attribute}<AppSelect value={attribute} ariaLabel={text.attributeLabel} onChange={value => setAttribute(value as AttributeChoice)} options={[
            { value: 'str', label: text.str }, { value: 'dex', label: text.dex }, { value: 'int', label: text.int },
          ]} /></label>
        </div>
        <details className="tree-planner-settings"><summary className="disclosure">{text.searchRules}</summary>
          <p className="tree-planner-hint">{text.scope}</p><p className="tree-planner-hint">{text.limits} {text.refresh}</p>
          <p className="tree-planner-hint">{tt('tree.campaignPoints')}</p>
        </details>
        {session.character && remaining !== null && mode === 'allocate' && remaining <= 0 && <p className="tree-planner-hint">
          {!context.canRefund && `${text.refundUnavailable} `}
          {canReallocate ? text.budgetSwitchMode : text.budgetFutureOnly}
        </p>}
        {mode === 'future' && <p className="tree-planner-hint">{text.futureHint}</p>}
        {mode === 'reallocate' && !context.canRefund && <p className="tree-planner-hint">{text.refundUnavailable} {text.budgetFutureOnly}</p>}
        {!session.character && <p className="tree-planner-hint">{text.needsBuild}</p>}
        {context.root === null && session.character && <p className="tree-planner-hint">{text.unavailable}</p>}
        <AppButton className="tree-planner-run" onClick={run} disabled={session.busy || Boolean(session.calcError) || !session.calc || progress !== null || !session.character
          || context.root === null || !searchAllowed}>{text.run}</AppButton>
        {tracking && !evaluated && !progress && !error && !session.calcError && context.root !== null
          && (searchAllowed || session.busy) && <p className="tree-planner-hint" role="status">{text.waiting}</p>}
        {progress && <OptimizerProgress {...progress} lang={lang} onCancel={cancel} />}
        {error && <p className="opt-error">{error}</p>}
        {evaluated && <PlannerResultsNotice count={evaluated.plans.length} lang={lang}>
          <details className="tree-planner-summary">
            <summary className="disclosure">{text.evaluationDetails}</summary>
            {evaluated.evaluated > 0 && <p>{text.baseline}{separator}{damage.label} {formatStat(damage.stat, currentStats[damage.stat] ?? 0, lang)} · {statNameLabel(lang, 'TotalEHP')} {formatStat('TotalEHP', currentStats.TotalEHP ?? 0, lang)}</p>}
            <p data-evaluated-count={evaluated.evaluated}>{text.evaluated}{separator}{evaluated.evaluated} · {text.data}{separator}{completed!.dataVersion ?? text.unknownVersion} · {text.tree}{separator}{completed!.treeVersion ?? text.defaultTree}</p>
            {damage.note && <p>{damage.note}</p>}
            <p>{evaluated.limited ? text.limited : text.bounded}</p>
            {evaluated.unmodeled > 0 && <p>{text.skipped}{separator}{evaluated.unmodeled}</p>}
            {evaluated.baselineUnsupported.length > 0 && <details className="tree-planner-diagnostics tree-planner-baseline-unsupported">
              <summary className="disclosure" data-disclosure-level="group">{text.baselineLimits}{separator}{evaluated.baselineUnsupported.length}</summary>
              <p className="tree-planner-hint">{text.baselineLimitsHint}</p>
              <UnincludedModifiers lines={evaluated.baselineUnsupported} lang={lang} />
            </details>}
            {evaluated.excluded.length > 0 && <details className="tree-planner-diagnostics">
              <summary className="disclosure" data-disclosure-level="group">{text.excluded}{separator}{evaluated.excluded.length}</summary>
              <p className="tree-planner-hint">{text.excludedHint}</p>
              <ol className="tree-planner-excluded">{evaluated.excluded.map(({ plan, unsupported, error: engineError }, index) => <li key={index}>
                <details>
                  <summary className="disclosure">{text.route}{separator}{nodeLabel(plan.target)} · {text.add} {plan.allocate.length} · {text.remove} {plan.deallocate.length}</summary>
                  <p>{text.add}{separator}{plan.allocate.map(id => `${nodeLabel(id)} #${id}`).join(' · ') || '—'}</p>
                  <p>{text.remove}{separator}{plan.deallocate.map(id => `${nodeLabel(id)} #${id}`).join(' · ') || '—'}</p>
                  {unsupported.length > 0 && <><p>{text.newLimits}</p>
                    <UnincludedModifiers lines={unsupported} lang={lang} /></>}
                  {engineError && <p className="opt-error">{text.engineError}{separator}{engineError}</p>}
                  {unsupported.length === 0 && !engineError && <p className="opt-error">{text.invalidStats}</p>}
                </details>
              </li>)}</ol>
            </details>}
          </details>
          {evaluated.plans.length === 0 && <p className="tree-planner-hint">{text.noResults}</p>}
          <ol className="tree-planner-results">
            {evaluated.plans.map((plan, index) => {
              const added = plan.allocate.length;
              const removed = plan.deallocate.length;
              const net = added - removed;
              const overflow = mode === 'future' && remaining !== null ? passivePlanOverflow(plan, remaining) : 0;
              const score = objective ? scoreOf(plan.stats, objective) : 0;
              const baselineScore = objective ? scoreOf(currentStats, objective) : 0;
              return <li key={index} className={preview === index ? 'is-previewed' : ''}>
                <div className="tree-plan-heading"><strong>{nodeLabel(plan.target)}</strong></div>
                <div className="tree-plan-metrics">
                  {objective && (objective.per || objective.secondaryStat) && Math.abs(score - baselineScore) >= 0.005 &&
                    <span>{text.score} <b className={score >= baselineScore ? 'is-positive' : 'is-negative'}>{delta(score, baselineScore)}</b></span>}
                  {metrics.filter(stat => Math.abs((plan.stats[stat] ?? 0) - (currentStats[stat] ?? 0)) >= 0.005).map(stat => <span key={stat}>
                    {stat === damage.stat ? damage.label : treeStatLabel(lang, stat)} <b className={(plan.stats[stat] ?? 0) >= (currentStats[stat] ?? 0) ? 'is-positive' : 'is-negative'}>{delta(plan.stats[stat] ?? 0, currentStats[stat] ?? 0)}</b>
                  </span>)}
                </div>
                <div className="tree-plan-points">
                  {added > 0 && <span data-change="add"><b aria-hidden="true">+</b> {text.addPoints.replace('{count}', String(added))}</span>}
                  {removed > 0 && <span data-change="remove"><b aria-hidden="true">−</b> {text.refundCount.replace('{count}', String(removed))}</span>}
                  {removed > 0 && <span>{net === 0 ? text.samePoints : (net < 0 ? text.savePoints : text.spendPoints).replace('{count}', String(Math.abs(net)))}</span>}
                </div>
                {overflow > 0 && <p className="tree-plan-budget">{text.futureOverflow.replace('{count}', String(overflow))}</p>}
                <details><summary className="disclosure">{text.routeDetails}</summary>
                  {added > 0 && <p>{text.add}{separator}{plan.allocate.map(id => `${nodeLabel(id)} #${id}`).join(' · ')}</p>}
                  {removed > 0 && <p>{text.remove}{separator}{plan.deallocate.map(id => `${nodeLabel(id)} #${id}`).join(' · ')}</p>}
                  {metrics.map(stat => <p key={stat}>
                    {stat === damage.stat ? damage.label : treeStatLabel(lang, stat)}{separator}{formatStat(stat, currentStats[stat] ?? 0, lang)} → <output data-stat={stat}>{formatStat(stat, plan.stats[stat] ?? 0, lang)}</output>
                  </p>)}
                </details>
                <div className="tree-plan-actions">
                  <AppButton data-route-preview-trigger aria-pressed={preview === index} onClick={() => show(index)}>{text.preview}</AppButton>
                  <AppButton disabled={session.busy} onClick={() => void apply(plan)}>{overflow > 0 ? text.applyFuture : text.apply}</AppButton>
                </div>
              </li>;
            })}
          </ol>
        </PlannerResultsNotice>}
        <JewelPassivePlanner planner={jewelPlanner} />
      </div>}
    </div>
  );
}
