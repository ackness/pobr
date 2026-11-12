import { colon } from '../../app/text';
import { DisclosureChevron } from '../shared/DisclosureChevron';
import { formatNumber, formatStat } from '../../lib/statDisplay';
import { UnincludedModifiers } from '../shared/UnincludedModifiers';
import { defaultTravelAttribute, treeStatLabel } from './treePresentation';
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
  mode: 'allocate' | 'reallocate';
  setMode: (value: 'allocate' | 'reallocate') => void;
  points: number;
  setPoints: (value: number) => void;
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
  apply: (plan: PassivePlan) => void;
  jewelPlanner: JewelPassivePlannerState;
}


export function useTreeOptimizer({ session, lang, active, nodes, nodeLabel, onLocate, onPreview, onRun, focusPlanner }: Props): TreeOptimizerState {
  const [expanded, setExpanded] = useState(Boolean(focusPlanner));
  const [mode, setMode] = useState<'allocate' | 'reallocate'>('allocate');
  const [points, setPoints] = useState(3);
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
  const baselineStats = useMemo(() => Object.fromEntries((session.calc?.stats ?? []).map(stat => [stat.id, stat.value ?? 0])), [session.calc]);
  const objective = useMemo(() => resolveDamageObjective(objectiveOf(goal, baselineStats), session.calc), [goal, baselineStats, session.calc]);
  const request = useMemo(() => session.currentRequest(), [session.currentRequest, session.stateVersion]);
  // Pair the full evaluation with its effective goal and authoritative jewel topology.
  const snapshotKey = useMemo(() => JSON.stringify([session.stateVersion, session.dataVersion, session.treeVersion,
    session.activeWeaponSet, request, session.calc?.tree_effects, mode, points, goal, attribute, objective]),
  [session.stateVersion, session.dataVersion, session.treeVersion, session.activeWeaponSet, request,
    session.calc?.tree_effects, mode, points, goal, attribute, objective]);
  const latestKeyRef = useRef(snapshotKey);
  latestKeyRef.current = snapshotKey;
  const activeRef = useRef(active);
  activeRef.current = active;
  const evaluated = completed?.key === snapshotKey ? completed.result : null;

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
      || !request || context.root === null || (mode === 'reallocate' && !context.canRefund)) return;
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
        const result = await planPassiveUpgrades({ request: prepared, context, points, mode, objective,
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
    context, nodes, attribute, points, mode, objective, runId]);

  const run = () => {
    if (!active || session.busy || session.calcError || !session.calc) return;
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
    setPreview(index);
    previewRef.current(index === null ? null : evaluated?.plans[index] ?? null);
  };
  const clearPreview = () => {
    setPreview(null);
    previewRef.current(null);
    jewelPlanner.clearPreview();
  };
  const apply = (plan: PassivePlan) => {
    if (!active || !completed || completed.key !== latestKeyRef.current || session.busy
      || JSON.stringify(session.currentRequest()) !== JSON.stringify(completed.request)
      || !completed.result.plans.some(candidate => candidate === plan)) return;
    const next = passivePlanAllocation(completed.preparedRequest, context, plan, points, mode);
    if (!next) return;
    // Apply the same full allocation and attribute choices used by the successful evaluation.
    stop();
    setCompleted(null);
    session.setAllocatedNodes(next.allocatedNodes, next.attributeChoices);
    // A successful atomic session edit increments stateVersion exactly once.
    // Later edits cannot revive this receipt, even if they restore the same allocation.
    setApplied({ version: session.stateVersion + 1, plan });
  };
  const jewelPlanner = useJewelPassivePlanner({ session, lang, nodes, nodeLabel, onLocate, active: active && expanded,
    mode, points, attribute, objective, onPreview, onStart: () => { cancel(); onRun?.(); } });
  return { session, lang, nodeLabel, containerRef, expanded, setExpanded, mode, setMode, points, setPoints,
    goal, setGoal, attribute, setAttribute, context, tracking, progress, error, completed, evaluated, preview,
    applied: applied?.version === session.stateVersion && !session.busy && !session.calcError ? applied.plan : null,
    run, cancel, show, clearPreview, apply, jewelPlanner };
}

export function TreeOptimizer({ planner }: { planner: TreeOptimizerState }) {
  const { session, lang, nodeLabel, containerRef, expanded, setExpanded, mode, setMode, points, setPoints,
    goal, setGoal, attribute, setAttribute, context, tracking, progress, error, completed, evaluated, preview, applied,
    run, cancel, show, apply, jewelPlanner } = planner;
  const text = messages(lang, 'treeOptimizer');
  const separator = colon(lang);
  const tt = bindT(lang);
  const number = (value: number) => formatNumber(value);
  const delta = (value: number, baseline: number) => baseline > 0 && Math.abs(value / baseline - 1) >= 0.0005
    ? `${value >= baseline ? '+' : ''}${formatNumber((value / baseline - 1) * 100, 1, 1)}%`
    : `${value >= baseline ? '+' : ''}${number(value - baseline)}`;
  const currentStats = evaluated?.baseline ?? {};
  const damage = useHeadlineDamage(session.calc, lang);

  return (
    <div className="tree-planner" ref={containerRef} id="passive-upgrades">
      <AppButton className="tree-planner-toggle disclosure" data-disclosure-level="section" aria-expanded={expanded} onClick={() => { if (expanded) cancel(); setExpanded(!expanded); }}>
        <DisclosureChevron />{text.title}
      </AppButton>
      {expanded && <div className="tree-planner-body">
        <p className="tree-planner-hint">{text.hint.replace('{points}', String(points))}</p>
        {applied && <p className="tree-planner-hint tree-planner-applied" role="status">
          <strong>{text.applied}</strong><br />
          {text.add} {applied.allocate.length}{separator}{applied.allocate.map(nodeLabel).join(' · ') || '—'}<br />
          {text.remove} {applied.deallocate.length}{separator}{applied.deallocate.map(nodeLabel).join(' · ') || '—'}
        </p>}
        <WeaponSetControl session={session} lang={lang} />
        <div className="tree-planner-controls">
          <label>{text.mode}<AppSelect value={mode} ariaLabel={text.mode} onChange={value => setMode(value as typeof mode)} options={[
            { value: 'allocate', label: text.allocate }, { value: 'reallocate', label: text.reallocate },
          ]} /></label>
          <label>{mode === 'allocate' ? text.points : text.refundPoints}<AppInput type="number" min={1} max={PASSIVE_POINT_LIMIT} value={points}
            onChange={event => { const value = Number(event.target.value); if (Number.isInteger(value) && value >= 1 && value <= PASSIVE_POINT_LIMIT) setPoints(value); }} /></label>
          <ObjectiveEditor damage={damage} statName={id => treeStatLabel(lang, id)} baseline={Object.fromEntries((session.calc?.stats ?? []).filter(stat => stat.value !== null).map(stat => [stat.id, stat.value!]))} value={goal} onChange={setGoal} lang={lang} />
          <label>{text.attribute}<AppSelect value={attribute} ariaLabel={text.attributeLabel} onChange={value => setAttribute(value as AttributeChoice)} options={[
            { value: 'str', label: text.str }, { value: 'dex', label: text.dex }, { value: 'int', label: text.int },
          ]} /></label>
        </div>
        <p className="tree-planner-hint">{tt('tree.campaignPoints')}</p>
        <details><summary className="disclosure">{messages(lang, 'nodePower').calculationDetails}</summary><p className="tree-planner-hint">{text.scope}</p><p className="tree-planner-hint">{text.limits} {text.refresh}</p></details>
        {mode === 'reallocate' && !context.canRefund && <p className="tree-planner-hint">{text.refundUnavailable}</p>}
        {!session.character && <p className="tree-planner-hint">{text.needsBuild}</p>}
        {context.root === null && session.character && <p className="tree-planner-hint">{text.unavailable}</p>}
        <AppButton className="tree-planner-run" onClick={run} disabled={session.busy || Boolean(session.calcError) || !session.calc || progress !== null || !session.character
          || context.root === null || (mode === 'reallocate' && !context.canRefund)}>{text.run}</AppButton>
        {tracking && !evaluated && !progress && !error && !session.calcError && context.root !== null
          && (mode === 'allocate' || context.canRefund) && <p className="tree-planner-hint" role="status">{text.waiting}</p>}
        {progress && <OptimizerProgress {...progress} lang={lang} onCancel={cancel} />}
        {error && <p className="opt-error">{error}</p>}
        {evaluated && <PlannerResultsNotice count={evaluated.plans.length} lang={lang}>
          <div className="tree-planner-summary">
            {evaluated.evaluated > 0 && <span>{text.baseline}{separator}{damage.label} {formatStat(damage.stat, currentStats[damage.stat] ?? 0, lang)} · {statNameLabel(lang, 'TotalEHP')} {formatStat('TotalEHP', currentStats.TotalEHP ?? 0, lang)}</span>}
            <details><summary className="disclosure">{messages(lang, 'nodePower').calculationDetails}</summary><span>{text.evaluated}{separator}{evaluated.evaluated}</span><p>{text.data}{separator}{completed!.dataVersion ?? text.unknownVersion} · {text.tree}{separator}{completed!.treeVersion ?? text.defaultTree}</p></details>
            {preview !== null && <AppButton onClick={() => show(null)}>{text.clear}</AppButton>}
          </div>
          {damage.note && <p className="tree-planner-hint">{damage.note}</p>}
          {evaluated.limited && <p className="tree-planner-hint">{text.limited}</p>}
          {evaluated.unmodeled > 0 && <p className="tree-planner-hint">{text.skipped}{separator}{evaluated.unmodeled}</p>}
          {preview !== null && <p className="tree-planner-hint" role="status">{text.previewHint}</p>}
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
          {evaluated.plans.length === 0 && <p className="tree-planner-hint">{text.noResults}</p>}
          <ol className="tree-planner-results">
            {evaluated.plans.map((plan, index) => <li key={index} className={preview === index ? 'is-previewed' : ''}>
              <div className="tree-plan-heading"><strong>{index + 1}. {nodeLabel(plan.target)}</strong>
                <span>{text.cost} {plan.allocate.length - plan.deallocate.length} · {text.add} {plan.allocate.length}{plan.deallocate.length > 0 ? ` · ${text.remove} ${plan.deallocate.length}` : ''}</span></div>
              <div className="tree-plan-metrics">
                {Math.abs(scoreOf(plan.stats, completed!.objective) - scoreOf(currentStats, completed!.objective)) >= 0.005 && <span>{text.score} <b>{delta(scoreOf(plan.stats, completed!.objective), scoreOf(currentStats, completed!.objective))}</b></span>}
                {[damage.stat, 'TotalEHP', 'Life'].filter(stat => Math.abs((plan.stats[stat] ?? 0) - (currentStats[stat] ?? 0)) >= 0.005).map(stat => <span key={stat}>
                  {stat === damage.stat ? damage.label : treeStatLabel(lang, stat)} <b className={(plan.stats[stat] ?? 0) >= (currentStats[stat] ?? 0) ? 'is-positive' : 'is-negative'}>{delta(plan.stats[stat] ?? 0, currentStats[stat] ?? 0)}</b>

                </span>)}
              </div>
              <details><summary className="disclosure">{text.route} · +{plan.allocate.length} {plan.allocate.slice(0, 2).map(nodeLabel).join(' · ')}{plan.allocate.length > 2 ? '…' : ''}{plan.deallocate.length > 0 ? ` · −${plan.deallocate.length} ${plan.deallocate.slice(0, 2).map(nodeLabel).join(' · ')}${plan.deallocate.length > 2 ? '…' : ''}` : ''}</summary><p>{text.add}{separator}{plan.allocate.map(nodeLabel).join(' · ') || '—'}</p>
                <p>{text.remove}{separator}{plan.deallocate.map(nodeLabel).join(' · ') || '—'}</p>
                {[damage.stat, 'TotalEHP', 'Life'].map(stat => <p key={stat}>
                  {stat === damage.stat ? damage.label : treeStatLabel(lang, stat)}{separator}{formatStat(stat, currentStats[stat] ?? 0, lang)} → <output data-stat={stat}>{formatStat(stat, plan.stats[stat] ?? 0, lang)}</output>
                </p>)}
              </details>
              <div className="tree-plan-actions"><AppButton onClick={() => show(index)}>{text.preview}</AppButton><AppButton disabled={session.busy} onClick={() => apply(plan)}>{text.apply}</AppButton></div>
            </li>)}
          </ol>
          <p className="tree-planner-hint">{text.bounded}</p>
        </PlannerResultsNotice>}
        <JewelPassivePlanner planner={jewelPlanner} />
      </div>}
    </div>
  );
}
