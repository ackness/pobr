import { useCallback, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { CalculateBuildRequest } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { planEquipmentUpgrades, EQUIPMENT_CANDIDATE_LIMIT, EQUIPMENT_MAX_REPLACEMENTS, type EquipmentCandidate, type EquipmentPlan, type EquipmentPlanningResult } from '../../lib/equipmentPlanner';
import { compareObjectiveStats, feasibleOf, type Objective } from '../../lib/optimize';
import { slotLabel, statNameLabel, type Lang } from '../../lib/i18n';
import { upgradeT } from '../../lib/upgradeText';
import type { TradeCatalog } from '../../lib/tradeOptimizer';

interface EquipmentSnapshot {
  key: string;
  result: EquipmentPlanningResult;
  request: CalculateBuildRequest;
  objective: Objective;
  catalog: TradeCatalog;
  dataVersion: BuildSession['dataVersion'];
  treeVersion: BuildSession['treeVersion'];
  candidates: EquipmentCandidate[];
  maxReplacements: number;
}

/** Candidate texts are the exact prepared items; plans apply all replacements in one edit. */
export function EquipmentPlanner({ session, lang, active, objective, catalog, candidates, onRemove }: {
  session: BuildSession; lang: Lang; active: boolean; objective: Objective; catalog: TradeCatalog | null;
  candidates: EquipmentCandidate[]; onRemove: (id: string) => void;
}) {
  const ut = (key: Parameters<typeof upgradeT>[1]) => upgradeT(lang, key);
  const [snapshot, setSnapshot] = useState<EquipmentSnapshot | null>(null);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState({ done: 0, total: 0 });
  const [error, setError] = useState('');
  const [maxReplacements, setMaxReplacements] = useState(2);
  const controller = useRef<AbortController | null>(null);
  const key = useMemo(() => JSON.stringify([session.activeWeaponSet, session.currentRequest(), candidates, maxReplacements,
    objective, session.dataVersion, session.treeVersion]),
  [session.currentRequest, session.activeWeaponSet, candidates, maxReplacements, objective, session.dataVersion, session.treeVersion]);
  const latest = useRef({ active, key, catalog });
  latest.current = { active, key, catalog };
  const previousIdentity = useRef({ key, catalog });
  const result = snapshot?.key === key && snapshot.catalog === catalog ? snapshot.result : null;
  const stop = useCallback(() => {
    controller.current?.abort(); controller.current = null;
    setBusy(false); setProgress({ done: 0, total: 0 });
  }, []);
  useLayoutEffect(() => {
    if (previousIdentity.current.key === key && previousIdentity.current.catalog === catalog) return;
    previousIdentity.current = { key, catalog };
    stop(); setSnapshot(null); setError('');
  }, [key, catalog, stop]);
  useLayoutEffect(() => { if (!active) stop(); }, [active, stop]);
  useLayoutEffect(() => () => stop(), [stop]);
  const format = (value: number) => value.toLocaleString(lang, { maximumFractionDigits: 2 });
  const reasonText = (reason: string) => ({
    'wrong-slot': ut('wrongSlot'), 'weapon-type': ut('weaponType'), 'unknown-base': ut('unknownBase'),
    'item-level': ut('itemLevel'), 'unidentified-item': ut('unidentified'), 'augment-limit': ut('augmentLimit'),
    'unknown-equipped-augments': ut('augmentUnverified'),
  }[reason] ?? reason);
  const usable = (plan: EquipmentPlan) => !plan.error && !plan.unsupported.length && !plan.warnings?.length;
  const plans = result ? [...result.plans.filter(plan => plan.candidateIds.length).sort((a, b) => Number(usable(b)) - Number(usable(a))
    || compareObjectiveStats(a.stats, b.stats, objective) || a.candidateIds.length - b.candidateIds.length).slice(0, 5),
    ...result.plans.filter(plan => !plan.candidateIds.length)] : [];
  const rejected = result?.plans.filter(plan => plan.candidateIds.length && (plan.error || plan.warnings?.length || plan.unsupported.length)) ?? [];

  const run = async () => {
    const request = session.currentRequest();
    const runKey = JSON.stringify([session.activeWeaponSet, request, candidates, maxReplacements,
      objective, session.dataVersion, session.treeVersion]);
    if (!active || !latest.current.active || busy || session.busy || !request || !catalog
      || latest.current.key !== runKey || latest.current.catalog !== catalog) return;
    stop();
    const abort = new AbortController(); controller.current = abort;
    const current = () => !abort.signal.aborted && controller.current === abort && latest.current.active
      && latest.current.key === runKey && latest.current.catalog === catalog;
    setBusy(true); setError(''); setSnapshot(null); setProgress({ done: 0, total: 0 });
    try {
      const result = await planEquipmentUpgrades({ request, candidates, maxReplacements, objective, signal: abort.signal,
        onProgress: (done, total) => { if (current()) setProgress({ done, total }); } }, { catalog });
      if (current()) setSnapshot({ key: runKey, result, request, objective, catalog,
        dataVersion: session.dataVersion, treeVersion: session.treeVersion, candidates, maxReplacements });
    } catch (error) {
      if (current()) setError(error instanceof Error ? error.message : String(error));
    } finally {
      if (controller.current === abort) { setBusy(false); setProgress({ done: 0, total: 0 }); controller.current = null; }
    }
  };
  const apply = (plan: EquipmentPlan) => {
    const currentKey = JSON.stringify([session.activeWeaponSet, session.currentRequest(), candidates, maxReplacements,
      objective, session.dataVersion, session.treeVersion]);
    if (!active || !latest.current.active || busy || session.busy || !result || snapshot?.key !== currentKey
      || snapshot.key !== latest.current.key || snapshot.catalog !== catalog || latest.current.catalog !== catalog
      || !result.plans.includes(plan) || !usable(plan) || !feasibleOf(plan.stats, objective) || !plan.items.length) return;
    session.setItems([...session.items.filter(item => !plan.items.some(next => next.slot === item.slot)), ...plan.items]);
    stop(); setSnapshot(null);
  };

  return <section className="equipment-planner" aria-labelledby="equipment-planner-title">
    <div className="equipment-planner-heading"><h4 id="equipment-planner-title">{ut('jointEquipment')}</h4>
      <span>{candidates.length} / {EQUIPMENT_CANDIDATE_LIMIT}</span></div>
    <p>{ut('jointEquipmentHint')}</p>
    <label>{ut('jointMaxChanges')} <select value={maxReplacements} disabled={!active || busy}
      onChange={event => setMaxReplacements(Number(event.target.value))}>
      {Array.from({ length: EQUIPMENT_MAX_REPLACEMENTS }, (_, index) => index + 1).map(count =>
        <option key={count} value={count}>{count}</option>)}</select></label>
    {!candidates.length && <p className="trade-notice">{ut('jointEmpty')}</p>}
    <ul className="equipment-candidates">{candidates.map(candidate => <li key={candidate.id}>
      <details><summary><strong>{candidate.label}</strong> · {slotLabel(lang, candidate.slot)}</summary><pre>{candidate.text}</pre></details>
      <button aria-label={`${ut('jointRemove')} ${candidate.label} · ${slotLabel(lang, candidate.slot)}`} disabled={!active || busy} onClick={() => onRemove(candidate.id)}>{ut('jointRemove')}</button>
    </li>)}</ul>
    {!!candidates.length && <div className="replacement-actions">
      <button className="trade-primary" disabled={!active || busy || session.busy || !catalog} onClick={() => void run()}>
        {maxReplacements === 2 ? ut('jointCompare') : maxReplacements === 1 ? ut('jointCompareOne')
          : `${ut('jointCompareMany')} ${maxReplacements} ${ut('jointItems')}`}</button>
      {busy && <><span role="status">{ut('comparingPositions')} {progress.done} / {progress.total}</span>
        <button onClick={stop}>{ut('cancelCompare')}</button></>}
    </div>}
    {error && <p role="alert" className="trade-error">{error}</p>}
    {result && <div className="equipment-plan-results" aria-live="polite">
      <p>{ut('jointEvaluated')} {result.evaluated} · {ut('jointTop')}</p>
      {result.limited && <p className="trade-notice">{ut('jointLimited')}</p>}
      <ol>{plans.map(plan => {
        const chosen = plan.candidateIds.map(id => candidates.find(candidate => candidate.id === id)!);
        const canApply = usable(plan) && feasibleOf(plan.stats, objective);
        return <li key={plan.candidateIds.join('/') || 'current'} className="equipment-plan">
          <h5>{chosen.length ? chosen.map(candidate => `${candidate.label} · ${slotLabel(lang, candidate.slot)}`).join(' + ') : ut('jointKeep')}</h5>
          {!plan.error && <div className="replacement-metrics">{['TotalDPS', 'TotalEHP', 'Life'].map(stat => {
            const before = result.baseline[stat] ?? 0, after = plan.stats[stat] ?? 0;
            const delta = after - before;
            return <div className="replacement-metric" key={stat}><span>{statNameLabel(lang, stat)}</span>
              <strong className={delta < 0 ? 'delta-neg' : delta > 0 ? 'delta-pos' : ''}>{delta > 0 ? '+' : ''}{format(before > 0 ? delta / before * 100 : delta)}{before > 0 ? '%' : ''}</strong>
              <span>{format(before)} → <b data-plan-stat={stat}>{format(after)}</b></span></div>;
          })}</div>}
          {!plan.error && <p className="equipment-plan-resists">{['FireResist', 'ColdResist', 'LightningResist', 'ChaosResist'].map(stat =>
            <span key={stat}>{statNameLabel(lang, stat)} {format(plan.stats[stat] ?? 0)}%</span>)}</p>}
          {!feasibleOf(plan.stats, objective) && <p className="delta-neg">{ut('constraintFail')}</p>}
          {(plan.error || !!plan.warnings?.length) && <p className="trade-notice">{ut('jointInvalid')} {plan.error ? reasonText(plan.error) : plan.warnings?.map(reasonText).join(', ')}</p>}
          {!!plan.unsupported.length && <details className="trade-notice"><summary>{ut('incomplete')} ({plan.unsupported.length})</summary>
            <ul>{plan.unsupported.map((line, index) => <li key={index}>{line}</li>)}</ul></details>}
          {!!chosen.length && <button className="trade-primary" disabled={!active || busy || session.busy || !canApply} onClick={() => apply(plan)}>{ut('jointApply')}</button>}
        </li>;
      })}</ol>
      {!!rejected.length && <details className="trade-notice"><summary>{ut('jointRejected')} ({rejected.length})</summary>
        <ul>{rejected.map(plan => <li key={plan.candidateIds.join('/')}>
          {plan.candidateIds.map(id => { const candidate = candidates.find(value => value.id === id)!;
            return `${candidate.label} · ${slotLabel(lang, candidate.slot)}`; }).join(' + ')} · {plan.error ? reasonText(plan.error) : plan.warnings?.map(reasonText).join(', ')}
          {!!plan.unsupported.length && <ul>{plan.unsupported.map((line, index) => <li key={index}>{line}</li>)}</ul>}
        </li>)}</ul></details>}
      <p className="replacement-requirement-note">{ut('requirementsNote')} {ut('jointLimits')}</p>
    </div>}
  </section>;
}
