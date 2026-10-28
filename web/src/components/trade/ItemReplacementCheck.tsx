import { useJobBeacon } from '../../v2/jobs/useJobBeacon';
import { AppButton, AppTextarea } from '../shared/controls';
import { useUi } from '../shared/UiContext';
import { usePreview } from '../shared/PreviewContext';
import { changesFromRecords } from '../../v2/preview/fromRecords';
import { words } from '../../v2/text';
import { craftT } from '../../lib/itemCraftingText';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { CalculateBuildRequest } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useLocalizedLines } from '../../hooks/useLocalizedLines';
import { compareReplacementPositions, pasteNotStarted, replacementAffixes, type ReplacementReport } from '../../lib/itemReplacement';
import { compareObjectiveStats, feasibleOf, type Objective } from '../../lib/optimize';
import type { TradeCatalog } from '../../lib/tradeOptimizer';
import { slotLabel, statNameLabel, type Lang } from '../../lib/i18n';
import { upgradeT } from '../../lib/upgradeText';
import type { AugmentSelection } from '../../lib/replacementAugments';
import { ReplacementAugmentPicker } from './ReplacementAugmentPicker';
import { ReplacementAffixEditor } from './ReplacementAffixEditor';
import { affixT } from '../../lib/marketAffixText';
import { EquipmentPlanner } from './EquipmentPlanner';
import { EQUIPMENT_CANDIDATE_LIMIT, type EquipmentCandidate } from '../../lib/equipmentPlanner';

interface ComparisonSnapshot {
  report: ReplacementReport;
  key: string;
  request: CalculateBuildRequest;
  input: string;
  objective: Objective;
  catalog: TradeCatalog;
  dataVersion: BuildSession['dataVersion'];
  treeVersion: BuildSession['treeVersion'];
  augmentPlans: Record<string, AugmentSelection>;
}

/** One visible clipboard flow; choosing a destination never mutates the build. */
export function ItemReplacementCheck({ session, lang, active, objective, catalog, jewelSockets, goalLabel, onEditGoal, initialItemText, candidate }: {
  session: BuildSession; lang: Lang; active: boolean; objective: Objective; catalog: TradeCatalog | null; jewelSockets: number[];
  goalLabel: string; onEditGoal: () => void;
  initialItemText?: string;
  candidate?: { kind: 'item' | 'jewel' };
}) {
  const { v2 } = useUi();
  const ut = (key: Parameters<typeof upgradeT>[1]) => upgradeT(lang, key);
  const [saved, setSaved] = useState(false);
  const [text, setText] = useState(initialItemText ?? '');
  const [snapshot, setSnapshot] = useState<ComparisonSnapshot | null>(null);
  const [chosenSlot, setChosenSlot] = useState<string | null>(null);
  const [applied, setApplied] = useState<{ slot: string; name: string; text: string; weaponSet: 1 | 2 } | null>(null);
  const appliedText = applied?.slot.startsWith('Jewel@')
    ? session.jewels.find(jewel => `Jewel@${jewel.socket_node}` === applied.slot)?.text
    : session.items.find(item => item.slot === applied?.slot)?.text ?? session.flasks.find(item => item.slot === applied?.slot)?.text;
  const [pendingPaste, setPendingPaste] = useState<{ input: string } | null>(initialItemText ? { input: initialItemText } : null);
  const [affixDirty, setAffixDirty] = useState(false);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  useJobBeacon(busy);
  const [augmentPlans, setAugmentPlans] = useState<Record<string, AugmentSelection>>({});
  const [equipmentCandidates, setEquipmentCandidates] = useState<EquipmentCandidate[]>([]);
  const candidateSequence = useRef(0);
  const controller = useRef<AbortController | null>(null);
  const timerRef = useRef<number | undefined>(undefined);
  const inputRef = useRef<HTMLTextAreaElement | null>(null);
  const contextKey = useMemo(() => JSON.stringify([
    session.activeWeaponSet, session.currentRequest(), objective, session.dataVersion, session.treeVersion, jewelSockets,
  ]), [session.currentRequest, session.activeWeaponSet, objective, session.dataVersion, session.treeVersion, jewelSockets]);
  const key = useMemo(() => JSON.stringify([contextKey, text, augmentPlans]), [contextKey, text, augmentPlans]);
  const latest = useRef({ active, contextKey, key, catalog });
  latest.current = { active, contextKey, key, catalog };
  const previousContext = useRef({ key: contextKey, catalog });
  // Hide incompatible receipts before effects run, including while this page is hidden.
  const report = snapshot?.key === key && snapshot.catalog === catalog ? snapshot.report : null;
  const preview = usePreview();
  const positions = report ? [...report.positions].sort((a, b) => compareObjectiveStats(a.stats, b.stats, objective) || a.slot.localeCompare(b.slot)) : [];
  const selected = positions.find(row => row.slot === chosenSlot) ?? positions[0];
  const best = positions[0];
  const recommended = report && best && !best.unsupported.length && !best.augments.limitWarning && feasibleOf(best.stats, objective)
    && compareObjectiveStats(best.stats, report.baseline, objective) < 0 ? best.slot : undefined;
  const affixes = replacementAffixes(selected?.lines ?? report?.lines ?? []);
  const translated = useLocalizedLines([report?.base.name ?? '', ...affixes.map(line => line.text)], lang);
  const nameLine = report?.lines.find(line => line.kind === 'name')?.text;
  const requiredLevel = Math.max(report?.requiredLevel ?? 0, ...(selected?.augments.runes.map(name =>
    selected.augments.info.options.find(option => option.name === name)?.required_level ?? 0) ?? []));
  const selectedItem = selected?.variant.set_items?.length === 1 ? selected.variant.set_items[0] : undefined;
  const alreadyAdded = !!report && !!selectedItem && equipmentCandidates.some(candidate =>
    candidate.itemId === report.text && candidate.slot === selectedItem.slot && candidate.text === selectedItem.text);
  const format = (value: number) => value.toLocaleString(lang, { maximumFractionDigits: 2 });
  const deltaText = (value: number) => `${value > 0 ? '+' : ''}${format(value)}`;
  const positionLabel = (slot: string) => slot.startsWith('Jewel@') ? `${ut('jewelPosition')} ${slot.slice(6)}` : slotLabel(lang, slot);
  const errorText = useCallback((message: string) => ({
    'unknown-required-level': craftT(lang, 'unknownRequirement'),
    'invalid-item': upgradeT(lang, 'invalidItem'), 'unknown-base': upgradeT(lang, 'unknownBase'),
    'wrong-slot': upgradeT(lang, 'wrongSlot'), 'weapon-type': upgradeT(lang, 'weaponType'),
    'item-level': upgradeT(lang, 'itemLevel'), 'unidentified-item': upgradeT(lang, 'unidentified'),
    'no-compatible-slot': upgradeT(lang, 'noCompatibleSlot'),
    'invalid-augments': upgradeT(lang, 'augmentInvalid'),
    'augment-limit': upgradeT(lang, 'augmentLimit'),
  }[message] ?? message), [lang]);
  const stop = useCallback(() => {
    controller.current?.abort(); controller.current = null;
    window.clearTimeout(timerRef.current); timerRef.current = undefined;
    setPendingPaste(null); setBusy(false);
  }, []);
  useLayoutEffect(() => {
    if (previousContext.current.key === contextKey && previousContext.current.catalog === catalog) return;
    previousContext.current = { key: contextKey, catalog };
    // A paste that has not begun is kept: the debounce below compares it under this context.
    if (!pasteNotStarted({ pending: pendingPaste !== null, hasReceipt: snapshot !== null, busy, failed: error !== '' })) stop();
    setSnapshot(null); setError(''); setAffixDirty(false);
  }, [contextKey, catalog, stop]);
  useLayoutEffect(() => { if (!active) { stop(); setApplied(null); } }, [active, stop]);
  useEffect(() => { if (report && inputRef.current) inputRef.current.scrollTop = 0; }, [report?.text]);
  useLayoutEffect(() => () => stop(), [stop]);

  const run = useCallback(async (input: string, plans: Record<string, AugmentSelection> = {}, keepSlot?: string) => {
    const request = session.currentRequest();
    const context = JSON.stringify([session.activeWeaponSet, request, objective, session.dataVersion, session.treeVersion, jewelSockets]);
    if (!active || !latest.current.active || session.busy || !request || !catalog
      || latest.current.contextKey !== context || latest.current.catalog !== catalog) return;
    stop();
    setApplied(null); setSaved(false);
    const abort = new AbortController(); controller.current = abort;
    const nextKey = JSON.stringify([context, input, plans]);
    const current = () => !abort.signal.aborted && controller.current === abort && latest.current.active
      && latest.current.key === nextKey && latest.current.catalog === catalog;
    setBusy(true); setError(''); setAugmentPlans(plans);
    if (!keepSlot) { setSnapshot(null); setChosenSlot(null); setAffixDirty(false); }
    else setChosenSlot(keepSlot);
    try {
      const next = await compareReplacementPositions(request, input, abort.signal, { catalog, jewelSocketNodes: jewelSockets, augmentPlans: plans, includeEmptySlots: !!candidate });
      if (current()) setSnapshot({ report: next, key: nextKey, request, input, objective, catalog,
        dataVersion: session.dataVersion, treeVersion: session.treeVersion, augmentPlans: plans });
    } catch (error) {
      if (current()) { setSnapshot(null); setAffixDirty(false); setError(errorText(error instanceof Error ? error.message : 'invalid-item')); }
    } finally {
      if (controller.current === abort) { setBusy(false); controller.current = null; }
    }
  }, [active, session.currentRequest, session.activeWeaponSet, session.dataVersion, session.treeVersion,
    session.busy, objective, catalog, jewelSockets, errorText, stop, candidate?.kind]);
  // Only an explicit paste owns a debounce; returning to this page never restarts work.
  useEffect(() => {
    if (!active || pendingPaste === null || session.busy || !catalog) return;
    const timer = window.setTimeout(() => {
      if (timerRef.current !== timer || !latest.current.active) return;
      timerRef.current = undefined;
      void run(pendingPaste.input);
    }, 150);
    timerRef.current = timer;
    return () => {
      window.clearTimeout(timer);
      if (timerRef.current === timer) timerRef.current = undefined;
    };
  }, [active, pendingPaste, session.busy, catalog, run]);

  const receiptCurrent = () => snapshot !== null && snapshot.catalog === catalog && latest.current.catalog === catalog
    && snapshot.key === latest.current.key
    && snapshot.key === JSON.stringify([JSON.stringify([session.activeWeaponSet, session.currentRequest(), objective,
      session.dataVersion, session.treeVersion, jewelSockets]), text, augmentPlans]);

  const apply = () => {
    if (!active || !latest.current.active || busy || affixDirty || session.busy || !report || !selected || !receiptCurrent()) return;
    const itemText = selected.variant.set_items?.find(item => item.slot === selected.slot)?.text
      ?? selected.variant.flasks?.find(item => item.slot === selected.slot)?.text
      ?? selected.variant.jewels?.find(jewel => `Jewel@${jewel.socket_node}` === selected.slot)?.text;
    if (itemText === undefined) return;
    // Apply the evaluated per-position payload, preserving all unrelated sources.
    if (selected.variant.jewels) session.setJewels(selected.variant.jewels);
    else if (selected.variant.flasks) session.setFlasks(selected.variant.flasks);
    else if (selected.variant.set_items) {
      const replacements = selected.variant.set_items;
      session.setItems([...session.items.filter(item => !replacements.some(row => row.slot === item.slot)), ...replacements]);
    }
    else return;
    setApplied({ slot: selected.slot, name: nameLine || translated[0] || report.base.name,
      text: itemText, weaponSet: session.activeWeaponSet });
    stop(); setSnapshot(null); setChosenSlot(null); setAffixDirty(false);
  };
  const addCandidate = () => {
    if (!active || !latest.current.active || busy || affixDirty || session.busy || !report || !selectedItem || !receiptCurrent() || alreadyAdded
      || equipmentCandidates.length >= EQUIPMENT_CANDIDATE_LIMIT) return;
    const candidate = { id: String(++candidateSequence.current), itemId: report.text,
      label: nameLine || translated[0] || report.base.name, slot: selectedItem.slot, text: selectedItem.text };
    setEquipmentCandidates(current => [...current, candidate]);
  };
  const candidateText = selected?.variant.set_items?.find(item => item.slot === selected.slot)?.text
    ?? selected?.variant.flasks?.find(item => item.slot === selected.slot)?.text
    ?? selected?.variant.jewels?.find(jewel => `Jewel@${jewel.socket_node}` === selected.slot)?.text ?? applied?.text ?? text;
  return <section className="upgrade-item-check ui-card" aria-labelledby="replacement-heading">
    {candidate ? <header><h3 id="replacement-heading">{craftT(lang, 'preview')}</h3><p>{craftT(lang, 'previewHint')}</p></header> : v2 ? <header className="replacement-heading"><div>
      <h3 id="replacement-heading">{words(lang, 'Paste an item to compare it', '粘贴物品来对比', '貼上物品來比較')}</h3><p>{ut('checkHint')}</p></div><div className="replacement-context"><span className="ui-badge">{ut('localComparison')}</span>
      <AppButton className="replacement-goal" aria-label={`${ut('comparisonGoal')}: ${goalLabel}`} onClick={onEditGoal}>{goalLabel} <span aria-hidden>↗</span></AppButton></div></header>
      : <header className="replacement-heading"><div><span className="trade-section-label">{ut('pasteEyebrow')}</span>
      <h3 id="replacement-heading">{ut('check')}</h3><p>{ut('checkHint')}</p></div><div className="replacement-context"><span className="ui-badge">{ut('localComparison')}</span>
      <AppButton className="replacement-goal" aria-label={`${ut('comparisonGoal')}: ${goalLabel}`} onClick={onEditGoal}>{goalLabel} <span aria-hidden>↗</span></AppButton></div></header>}
    <div className="replacement-input"><label htmlFor="replacement-text">{ut('itemText')}</label>
      <AppTextarea readOnly={!!candidate} ref={inputRef} id="replacement-text" rows={report ? 3 : 5} value={text} placeholder={ut('pastePlaceholder')}
        onChange={event => { if (candidate || !latest.current.active) return; stop(); setApplied(null); setSnapshot(null); setChosenSlot(null); setAffixDirty(false); setAugmentPlans({}); setText(event.target.value); }}
        onPaste={event => { if (candidate || !latest.current.active) return; const copied = event.clipboardData.getData('text/plain'); if (!copied) return;
          event.preventDefault(); stop(); setApplied(null); setSnapshot(null); setAugmentPlans({}); setChosenSlot(null); setAffixDirty(false); setText(copied); setPendingPaste({ input: copied }); }} />
      <div className="replacement-actions"><AppButton className="trade-primary" disabled={!active || busy || affixDirty || session.busy || !catalog || !text.trim()} onClick={() => void run(text, augmentPlans, chosenSlot ?? undefined)}>
        {busy ? ut('comparingPositions') : ut('compare')}</AppButton>
        {busy && <AppButton onClick={stop}>{ut('cancelCompare')}</AppButton>}
        {!candidate && <span>{ut('autoPositions')}</span>}</div>
      {!candidate && <p className="replacement-copy-links"><a href="/userscripts/pobr-market-copy.user.js" target="_blank" rel="noreferrer">{ut('copyScript')}</a>
        <a href="/userscripts/index.html" target="_blank" rel="noreferrer">{ut('copyScriptHelp')}</a></p>}
    </div>
    {candidate && <div className="replacement-actions"><AppButton disabled={busy || session.busy || affixDirty || saved || !active} onClick={() => {
      session.saveLibraryItem(candidate.kind, candidateText, selected?.slot.startsWith('Jewel@') ? undefined : selected?.slot); setSaved(true);
    }}>{craftT(lang, saved ? 'saved' : 'save')}</AppButton></div>}
    {error && <p role="alert" className="trade-error">{error}</p>}
    {active && applied && !session.busy && applied.weaponSet === session.activeWeaponSet && appliedText === applied.text && <p role="status" className="trade-notice replacement-applied">
      {ut('applied')} · {applied.name} · {positionLabel(applied.slot)}
    </p>}
    {report && selected && <div className="replacement-content" aria-busy={busy}>
      <div className="replacement-candidate">
      <aside className="replacement-parsed" aria-label={ut('parsedItem')}>
        <span className="trade-section-label">{ut('parsedItem')}</span><h4>{nameLine || translated[0]}</h4>
        <p>{translated[0]} · {ut('requiredLevel')} {requiredLevel}</p>
        <div className="replacement-affixes"><span className="replacement-affix-count">{ut('parsedAffixes')} · {affixes.length}</span>
          {affixes.map((line, index) => <div key={index} className={`replacement-affix replacement-affix--${line.kind}`}>
            <span>{translated[index + 1] ?? line.text}</span>{line.tier !== undefined ? <small>T{line.tier}</small> : line.kind !== 'explicit' && <small>{ut(line.kind === 'rune' ? 'affixRune' : line.kind === 'enchant' ? 'affixEnchant' : 'affixImplicit')}</small>}
          </div>)}{!affixes.length && <p>{ut('noAffixes')}</p>}
        </div>
        {report.lines.some(line => line.kind === 'class_req') && <p className="trade-notice">{report.lines.filter(line => line.kind === 'class_req').map(line => line.text).join('\n')}</p>}
        <p className="replacement-requirement-note">{ut('requirementsNote')}</p>
      </aside>
      {!candidate && catalog && <ReplacementAffixEditor key={report.text} report={report} catalog={catalog} characterLevel={session.currentRequest()?.character?.level ?? 1} lang={lang}
        disabled={!active || busy || session.busy} onDirtyChange={setAffixDirty} onCompare={input => {
          if (!latest.current.active) return;
          setAffixDirty(false); setText(input); void run(input, augmentPlans, selected.slot);
        }} />}
      <ReplacementAugmentPicker key={selected.slot} plan={selected.augments} level={session.currentRequest()?.character?.level ?? 1} lang={lang} disabled={!active || busy || affixDirty || session.busy}
        onChange={selection => { const plans = { ...augmentPlans, [selected.slot]: selection }; void run(text, plans, selected.slot); }} />
      </div>
      <div className="upgrade-replacement-result" aria-live="polite">
        <div className="replacement-destinations" role="group" aria-label={ut('chooseReplacement')}>
          {positions.map(row => <AppButton key={row.slot} disabled={busy} className="replacement-position" aria-pressed={selected.slot === row.slot} onClick={() => { setChosenSlot(row.slot); setSaved(false); }}
            onMouseEnter={preview ? () => preview.showChanges(`${positionLabel(row.slot)} ← ${ut('replacementOutcome')}`, changesFromRecords(report.baseline, row.stats)) : undefined}
            onMouseLeave={preview?.clear} onFocus={preview ? () => preview.showChanges(`${positionLabel(row.slot)} ← ${ut('replacementOutcome')}`, changesFromRecords(report.baseline, row.stats)) : undefined} onBlur={preview?.clear}>
            <strong>{positionLabel(row.slot)}</strong>{recommended === row.slot && <span className="replacement-recommended">{ut('recommendedPosition')}</span>}
            <span className={(row.stats.TotalDPS ?? 0) < (report.baseline.TotalDPS ?? 0) ? 'delta-neg' : ''}>DPS {deltaText((row.stats.TotalDPS ?? 0) - (report.baseline.TotalDPS ?? 0))}</span>
            <span className={(row.stats.TotalEHP ?? 0) < (report.baseline.TotalEHP ?? 0) ? 'delta-neg' : ''}>EHP {deltaText((row.stats.TotalEHP ?? 0) - (report.baseline.TotalEHP ?? 0))}</span>
          </AppButton>)}
        </div>
        <div className="replacement-verdict"><span className="trade-section-label">{ut('replacementOutcome')}</span>
        <strong className={selected.unsupported.length || selected.augments.limitWarning ? '' : compareObjectiveStats(selected.stats, report.baseline, objective) < 0 ? 'delta-pos' : 'delta-neg'}>
          {selected.augments.limitWarning ? ut('augmentUnverified') : selected.unsupported.length ? ut('partialReplacement') : ut(compareObjectiveStats(selected.stats, report.baseline, objective) < 0 ? 'better' : 'worse')}</strong>
        {(selected.stats.TotalDPS ?? 0) < (report.baseline.TotalDPS ?? 0) && <p className="delta-neg">{ut('dpsDrop')}</p>}
        {!feasibleOf(selected.stats, objective) && <p className="delta-neg">{ut('constraintFail')}</p>}
        </div>
        <div className="replacement-metrics">{['TotalDPS', 'TotalEHP'].map(stat => {
          const before = report.baseline[stat] ?? 0, after = selected.stats[stat] ?? 0, delta = after - before;
          return <div className="replacement-metric" key={stat}><span>{statNameLabel(lang, stat)}</span>
            <strong className={delta < 0 ? 'delta-neg' : delta > 0 ? 'delta-pos' : ''}>{before > 0 ? `${deltaText(delta / before * 100)}%` : deltaText(delta)}</strong>
            <span>{format(before)} <span aria-hidden>→</span> <b>{format(after)}</b></span></div>;
        })}</div>
        <div className="replacement-table-wrap"><table><thead><tr><th>{positionLabel(selected.slot)}</th><th>{ut('before')}</th><th>{ut('after')}</th><th>{ut('change')}</th></tr></thead><tbody>
          {['TotalDPS', 'TotalEHP', 'Life', 'FireResist', 'ColdResist', 'LightningResist', 'ChaosResist'].map(stat => {
            const before = report.baseline[stat] ?? 0, after = selected.stats[stat] ?? 0, delta = after - before;
            return <tr key={stat}><th>{statNameLabel(lang, stat)}</th><td>{format(before)}</td><td>{format(after)}</td><td className={delta < 0 ? 'delta-neg' : delta > 0 ? 'delta-pos' : ''}>{deltaText(delta)}</td></tr>;
          })}</tbody></table></div>
        {!!selected.unsupported.length && <details className="trade-notice replacement-unsupported"><summary>{ut('incomplete')} ({selected.unsupported.length})</summary><ul>{selected.unsupported.map((line, index) => <li key={index}>{line}</li>)}</ul></details>}
        {!!report.rejected.length && <p className="trade-notice">{report.rejected.map(row => `${positionLabel(row.slot)}: ${errorText(row.reason)}`).join('\n')}</p>}
        <div className="replacement-apply"><AppButton className="trade-primary" disabled={!active || busy || affixDirty || session.busy} onClick={apply}>{busy ? ut('comparingPositions') : `${ut('apply')} · ${positionLabel(selected.slot)}`}</AppButton><span>{affixDirty ? affixT(lang, 'dirty') : ut('applyHint')}</span></div>
        {!candidate && selectedItem && <div className="replacement-actions"><AppButton disabled={!active || busy || affixDirty || session.busy || alreadyAdded || equipmentCandidates.length >= EQUIPMENT_CANDIDATE_LIMIT}
          onClick={addCandidate}>{ut(alreadyAdded ? 'jointAdded' : 'jointAdd')}</AppButton>
          <span>{equipmentCandidates.length} / {EQUIPMENT_CANDIDATE_LIMIT}</span></div>}
      </div>
    </div>}
    {!candidate && (v2 ? <details className="equipment-fold" open={equipmentCandidates.length > 0}>
      <summary><span>{ut('jointEquipment')}</span><small>{equipmentCandidates.length} / {EQUIPMENT_CANDIDATE_LIMIT}</small></summary>
      <EquipmentPlanner session={session} lang={lang} active={active} objective={objective} catalog={catalog} candidates={equipmentCandidates}
        onRemove={id => setEquipmentCandidates(current => current.filter(candidate => candidate.id !== id))} /></details>
      : <EquipmentPlanner session={session} lang={lang} active={active} objective={objective} catalog={catalog} candidates={equipmentCandidates}
      onRemove={id => setEquipmentCandidates(current => current.filter(candidate => candidate.id !== id))} />)}
  </section>;
}
