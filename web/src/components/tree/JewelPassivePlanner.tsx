import { DisclosureChevron } from '../shared/DisclosureChevron';
import { formatStatValue, statFormat, formatNumber } from '../../lib/statDisplay';
import { useHeadlineDamage } from '../../hooks/useMainMinion';
import { useJobBeacon } from '../../app/jobs/useJobBeacon';
import { AppButton, AppTextarea } from '../shared/controls';
import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { AttributeChoice, CalculateBuildRequest, PassiveNode } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { messages, statNameLabel, type Lang } from '../../lib/i18n';
import { normalizeCopiedItem } from '../../lib/itemReplacement';
import {
  JEWEL_PASSIVE_CANDIDATE_LIMIT, planJewelPassiveUpgrades,
  type JewelPassiveCandidate, type JewelPassivePlan, type JewelPassivePlannerResult,
} from '../../lib/jewelPassivePlanner';
import { allocationRoutes } from '../../lib/passiveGraph';
import { passivePlanningContext, withTravelAttributes, type PassivePlan } from '../../lib/passivePlanner';
import { feasibleOf, type Objective } from '../../lib/optimize';
import { AppSelect } from '../shared/AppSelect';
import { PlannerResultsNotice } from './PlannerResultsNotice';
import { OptimizerProgress } from '../shared/OptimizerControls';


interface Props {
  session: BuildSession; lang: Lang; nodes: PassiveNode[]; nodeLabel: (id: number) => string;
  onLocate?: (id: number) => void;
  active: boolean;
  mode: 'allocate' | 'reallocate'; points: number; attribute: AttributeChoice; objective: Objective;
  onPreview: (plan: PassivePlan | null) => void; onStart: () => void;
}

interface JewelPassiveReceipt {
  key: string;
  request: CalculateBuildRequest;
  objective: Objective;
  candidates: JewelPassiveCandidate[];
  result: JewelPassivePlannerResult;
}

export interface JewelPassivePlannerState {
  session: BuildSession;
  lang: Lang;
  nodeLabel: (id: number) => string;
  onLocate?: (id: number) => void;
  raw: string;
  setRaw: (value: string) => void;
  selectedSocket: string;
  setSocket: (value: string) => void;
  sockets: { node: PassiveNode; cost: number }[];
  candidates: JewelPassiveCandidate[];
  remove: (id: string) => void;
  completed: JewelPassiveReceipt | null;
  result: JewelPassivePlannerResult | null;
  progress: { done: number; total: number } | null;
  error: string;
  preview: number | null;
  applied: { plan: JewelPassivePlan; candidate: JewelPassiveCandidate | null } | null;
  exclusive: number[];
  add: () => void;
  run: () => Promise<void>;
  apply: (plan: JewelPassivePlan) => void;
  stop: () => void;
  show: (index: number | null) => void;
  /** Drops the joint route from the tree without touching the results. */
  clearPreview: () => void;
}

export function useJewelPassivePlanner({ session, lang, nodes, nodeLabel, onLocate, active, mode, points, attribute, objective, onPreview, onStart }: Props): JewelPassivePlannerState {
  const text = messages(lang, 'jewelPlanner');
  const [raw, setRaw] = useState('');
  const [socket, setSocket] = useState('');
  const [candidates, setCandidates] = useState<JewelPassiveCandidate[]>([]);
  const [completed, setCompleted] = useState<JewelPassiveReceipt | null>(null);
  const [applied, setApplied] = useState<{
    version: number; plan: JewelPassivePlan; candidate: JewelPassiveCandidate | null;
  } | null>(null);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  useJobBeacon(progress !== null);
  const [error, setError] = useState('');
  const [preview, setPreview] = useState<number | null>(null);
  const controller = useRef<AbortController | null>(null);
  const sequence = useRef(0);
  const previewRef = useRef(onPreview);
  previewRef.current = onPreview;
  const request = session.currentRequest();
  const exclusive = session.weaponSwap?.exclusive_nodes.flat() ?? [];
  const ascendancy = session.treeMeta?.classes.flatMap(entry => entry.ascendancies ?? [])
    .find(entry => entry.name === session.character?.ascendancy_name)?.id ?? session.character?.ascendancy_name;
  const context = useMemo(() => passivePlanningContext(nodes, session.allocatedNodes, session.character?.class_name,
    session.weaponSwap?.exclusive_nodes.flat() ?? [], session.jewels.map(jewel => jewel.socket_node), ascendancy, session.calc?.tree_effects),
  [nodes, session.allocatedNodes, session.character?.class_name, session.weaponSwap, session.jewels, ascendancy, session.calc]);
  const sockets = useMemo(() => {
    const routes = allocationRoutes(context.graph, context.allocated, context.root, context.grants, points);
    return nodes.filter(node => node.kind === 'jewel_socket' && context.byId.has(node.skill) && routes.has(node.skill))
      .map(node => ({ node, cost: routes.get(node.skill)!.length }))
      .sort((a, b) => a.cost - b.cost || a.node.skill - b.node.skill);
  }, [context, nodes, points]);
  const selectedSocket = sockets.some(entry => String(entry.node.skill) === socket) ? socket : String(sockets[0]?.node.skill ?? '');
  const key = JSON.stringify([session.stateVersion, session.dataVersion, session.treeVersion,
    session.activeWeaponSet, request, session.calc?.tree_effects, candidates, mode, points, attribute, objective]);
  const latestKey = useRef(key);
  latestKey.current = key;
  const activeRef = useRef(active);
  activeRef.current = active;
  const result = completed?.key === key ? completed.result : null;
  const stop = () => {
    controller.current?.abort(); controller.current = null;
    setProgress(null); setPreview(null); previewRef.current(null);
  };
  useLayoutEffect(() => {
    stop();
    setCompleted(null); setError('');
  }, [key]);
  useLayoutEffect(() => {
    if (!active) { stop(); setError(''); }
  }, [active]);
  useLayoutEffect(() => () => {
    controller.current?.abort(); controller.current = null;
  }, []);

  const add = () => {
    if (!active || !selectedSocket || candidates.length >= JEWEL_PASSIVE_CANDIDATE_LIMIT) return;
    try {
      const normalized = normalizeCopiedItem(raw);
      if (candidates.some(candidate => candidate.jewel.socket_node === Number(selectedSocket) && candidate.jewel.text === normalized)) return;
      const lines = normalized.split('\n');
      const rarity = lines.findIndex(line => /^Rarity:/i.test(line));
      const label = lines[rarity + 1] || text.text;
      setCandidates(previous => [...previous, { id: String(++sequence.current), label,
        jewel: { socket_node: Number(selectedSocket), text: normalized } }]);
      setError('');
    } catch { setError(text.invalid); }
  };
  const run = async () => {
    if (!active || !request || session.busy || session.calcError || !session.calc || exclusive.length || !candidates.length) return;
    onStart();
    controller.current?.abort();
    const abort = new AbortController(); controller.current = abort;
    setError(''); setCompleted(null); setPreview(null); onPreview(null); setProgress({ done: 0, total: 0 });
    const preparedRequest = withTravelAttributes(request, nodes, attribute);
    try {
      const result = await planJewelPassiveUpgrades({ request: preparedRequest, nodes,
        className: session.character?.class_name, ascendancy, exclusiveNodes: exclusive, candidates, points, mode, objective,
        signal: abort.signal, onProgress: (done, total) => {
          if (activeRef.current && !abort.signal.aborted && controller.current === abort && latestKey.current === key) setProgress({ done, total });
        } });
      if (activeRef.current && !abort.signal.aborted && controller.current === abort && !result.aborted && latestKey.current === key) {
        setCompleted({ key, request, objective, candidates, result });
      }
    } catch (error) {
      if (activeRef.current && !abort.signal.aborted && controller.current === abort && latestKey.current === key) setError(error instanceof Error ? error.message : String(error));
    } finally {
      if (controller.current === abort) { controller.current = null; setProgress(null); }
    }
  };
  const apply = (plan: JewelPassivePlan) => {
    if (!active || !completed || completed.key !== latestKey.current || session.busy
      || JSON.stringify(session.currentRequest()) !== JSON.stringify(completed.request)
      || !feasibleOf(plan.stats, completed.objective)) return;
    // The guard compares the original build, not travel choices prepared for evaluation.
    if (!session.applyTreePlan(completed.request, plan)) { setError(text.stale); return; }
    // Match the ordinary planner's receipt lifetime to the successful atomic edit.
    setApplied({ version: session.stateVersion + 1, plan,
      candidate: completed.candidates.find(candidate => candidate.id === plan.candidateId) ?? null });
    stop(); setCompleted(null);
  };
  const show = (index: number | null) => {
    if (!active || completed?.key !== latestKey.current || (index !== null && !result?.plans[index])) return;
    setPreview(index); onPreview(index === null ? null : result!.plans[index]);
  };
  const clearPreview = () => { setPreview(null); previewRef.current(null); };
  const remove = (id: string) => setCandidates(values => values.filter(value => value.id !== id));

  return { session, lang, nodeLabel, onLocate, raw, setRaw, selectedSocket, setSocket, sockets,
    applied: applied?.version === session.stateVersion && !session.busy && !session.calcError ? applied : null,
    candidates, remove, completed, result, progress, error, preview, exclusive, add, run, apply, stop, show, clearPreview };
}

export function JewelPassivePlanner({ planner }: { planner: JewelPassivePlannerState }) {
  const { session, lang, nodeLabel, onLocate, raw, setRaw, selectedSocket, setSocket, sockets,
    candidates, remove, completed, result, progress, error, preview, exclusive, applied, add, run, apply, stop, show } = planner;
  const text = messages(lang, 'jewelPlanner');
  const damage = useHeadlineDamage(session.calc, lang);
  const colon = lang === 'en-US' ? ': ' : '：';
  const format = (value: number) => formatNumber(value);
  const delta = (value: number, baseline: number) => baseline === 0 ? `${value >= 0 ? '+' : ''}${format(value)}`
    : `${value >= baseline ? '+' : ''}${format((value / baseline - 1) * 100)}%`;
  const [open, setOpen] = useState(true);
  return <section className="jewel-passive-planner" aria-label={text.title}>
    <AppButton className="tree-planner-toggle disclosure" data-disclosure-level="section" aria-expanded={open} onClick={() => { if (open) stop(); setOpen(!open); }}><DisclosureChevron />{text.title}</AppButton>
    {open && <><p className="tree-planner-hint">{text.hint}</p>
    {applied && <p className="tree-planner-hint jewel-plan-applied" role="status">
      <strong>{text.applied}</strong><br />
      {applied.candidate
        ? `${applied.candidate.label} · ${nodeLabel(applied.candidate.jewel.socket_node)}`
        : text.current}<br />
      {text.addNodes} {applied.plan.allocate.length}{colon}{applied.plan.allocate.map(nodeLabel).join(' · ') || '—'}<br />
      {text.removeNodes} {applied.plan.deallocate.length}{colon}{applied.plan.deallocate.map(nodeLabel).join(' · ') || '—'}
    </p>}
    <label className="jewel-plan-input">{text.text}<AppTextarea aria-label={text.text} value={raw} onChange={event => setRaw(event.target.value)} rows={5} /></label>
    <div className="tree-planner-controls"><label>{text.socket}<AppSelect value={selectedSocket} ariaLabel={text.socket}
      onChange={setSocket} disabled={!sockets.length} placeholder={text.noSockets} options={sockets.map(({ node, cost }) => ({ value: String(node.skill),
        label: nodeLabel(node.skill), hint: `${text.socketCost}${colon}${cost}` }))} /></label>
      <AppButton disabled={!selectedSocket || !onLocate} onClick={() => onLocate?.(Number(selectedSocket))}>{text.locate}</AppButton>
      <AppButton disabled={!raw.trim() || !selectedSocket || Boolean(progress) || candidates.length >= JEWEL_PASSIVE_CANDIDATE_LIMIT} onClick={add}>{text.add}</AppButton>
      <span>{candidates.length} / {JEWEL_PASSIVE_CANDIDATE_LIMIT}</span></div>
    {candidates.length >= JEWEL_PASSIVE_CANDIDATE_LIMIT && <p className="tree-planner-hint">{text.full}</p>}
    {!candidates.length && <p className="tree-planner-hint">{text.empty}</p>}
    <ul className="jewel-plan-candidates">{candidates.map(candidate => <li key={candidate.id}>
      <details><summary className="disclosure" data-disclosure-level="inline">{candidate.label} · {nodeLabel(candidate.jewel.socket_node)}</summary><pre>{candidate.jewel.text}</pre></details>
      <AppButton disabled={!onLocate} onClick={() => onLocate?.(candidate.jewel.socket_node)}>{text.locate}</AppButton>
      <AppButton disabled={Boolean(progress)} onClick={() => remove(candidate.id)}>{text.remove}</AppButton>
    </li>)}</ul>
    <details><summary className="disclosure" data-disclosure-level="inline">{messages(lang, 'nodePower').calculationDetails}</summary><p className="tree-planner-hint">{text.limit}</p></details>
    {exclusive.length > 0 && <p className="tree-planner-hint">{text.exclusive}</p>}
    <AppButton className="tree-planner-run" disabled={!candidates.length || session.busy || Boolean(session.calcError) || !session.calc || Boolean(progress) || exclusive.length > 0}
      onClick={() => void run()}>{text.run}</AppButton>
    {progress && <OptimizerProgress {...progress} lang={lang} onCancel={stop} />}
    {error && <p className="opt-error" role="alert">{error}</p>}
    {result && <PlannerResultsNotice count={result.plans.length} lang={lang}><div className="jewel-plan-results">
      <p className="tree-planner-hint">{text.evaluated}{colon}{result.evaluated}</p>
      {damage.note && <p className="tree-planner-hint">{damage.note}</p>}
      {result.limited && <p className="tree-planner-hint">{text.bounded}</p>}
      {result.unmodeled > 0 && <p className="tree-planner-hint">{text.skipped}{colon}{result.unmodeled}</p>}
      {result.diagnostics.length > 0 && <details><summary className="disclosure" data-disclosure-level="group">{text.diagnostics}</summary><ul>{result.diagnostics.map((reason, index) => <li key={index}>{reason}</li>)}</ul></details>}
      {!result.plans.length && <p>{text.none}</p>}
      {preview !== null && <AppButton onClick={() => show(null)}>{text.clear}</AppButton>}
      <ol className="tree-planner-results">{result.plans.map((plan, index) => <li key={index} className={preview === index ? 'is-previewed' : ''}>
        <div className="tree-plan-heading"><strong>{plan.candidateId === null ? text.current : plan.candidateLabel}</strong>
          <span>{plan.candidateId !== null && <>{nodeLabel(completed!.candidates.find(candidate => candidate.id === plan.candidateId)!.jewel.socket_node)} · </>}{text.addNodes} {plan.allocate.length} · {text.removeNodes} {plan.deallocate.length}</span></div>
        <div className="tree-plan-metrics">{[damage.stat, 'TotalEHP', 'Life'].map(stat => <span key={stat}>
          {stat === damage.stat ? damage.label : statNameLabel(lang, stat)} <b>{delta(plan.stats[stat] ?? 0, result.baseline[stat] ?? 0)}</b>
          <small>{formatStatValue(result.baseline[stat] ?? 0, statFormat(stat), lang)} → <output data-stat={stat}>{formatStatValue(plan.stats[stat] ?? 0, statFormat(stat), lang)}</output></small>
        </span>)}</div>
        <details><summary className="disclosure" data-disclosure-level="inline">{text.route}</summary><p>{text.addNodes}{colon}{plan.allocate.map(nodeLabel).join(' · ') || '—'}</p>
          <p>{text.removeNodes}{colon}{plan.deallocate.map(nodeLabel).join(' · ') || '—'}</p>
          {plan.candidateId !== null && <pre>{completed!.candidates.find(candidate => candidate.id === plan.candidateId)?.jewel.text}</pre>}</details>
        <div className="tree-plan-actions"><AppButton onClick={() => show(index)}>{text.preview}</AppButton>
          <AppButton disabled={session.busy || !feasibleOf(plan.stats, completed!.objective)} onClick={() => apply(plan)}>{text.apply}</AppButton></div>
      </li>)}</ol>
    </div></PlannerResultsNotice>}
    </>}
  </section>;
}
