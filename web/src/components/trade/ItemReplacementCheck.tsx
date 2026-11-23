import { UnincludedModifiers } from '../shared/UnincludedModifiers';
import { SwapGainCard } from '../items/SwapGainCard';
import { formatNumber, formatSignedNumber, formatStat } from '../../lib/statDisplay';
import { useNavigateToPage } from '../../app/pages/navigate';
import { useJobBeacon } from '../../app/jobs/useJobBeacon';
import { AppButton, AppTextarea } from '../shared/controls';
import { usePreview } from '../shared/PreviewContext';
import { changesFromRecords } from '../../app/preview/fromRecords';
import { colon } from '../../app/text';
import { itemNameLabel } from '../../lib/itemCrafting';
import { craftT } from '../../lib/itemCraftingText';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { CalculateBuildRequest } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useLocalizedLines, useLocalizedNames } from '../../hooks/useLocalizedLines';
import { isContinuation } from '../../lib/continuedLines';
import { compareReplacementPositions, pasteNotStarted, replacementAffixes, type ReplacementReport } from '../../lib/itemReplacement';
import { compareObjectiveStats, feasibleOf, minimumDeficit, type Objective } from '../../lib/optimize';
import type { TradeCatalog } from '../../lib/tradeOptimizer';
import { slotLabel, statNameLabel, t, type Lang } from '../../lib/i18n';
import { useHeadlineDamage } from '../../hooks/useMainMinion';
import type { DamageHeadline } from '../../lib/mainMinion';
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

interface AppliedReplacement {
  scope: BuildSession['calculationScope'];
  version: number;
  annotations: BuildSession['annotations'];
  weaponSet: 1 | 2;
  slot: string;
  name: string;
  text: string;
  installed: (session: BuildSession) => boolean;
}

/** One visible clipboard flow; choosing a destination never mutates the build. */
export function ItemReplacementCheck({ session, lang, active, objective, catalog, jewelSockets, goalLabel, onEditGoal, initialItemText, candidate, socketLabel, actionClassName = 'trade-primary', damage: providedDamage }: {
  session: BuildSession; lang: Lang; active: boolean; objective: Objective; catalog: TradeCatalog | null; jewelSockets: number[];
  goalLabel: string; onEditGoal: () => void;
  socketLabel?: (id: number) => string;
  initialItemText?: string;
  candidate?: { kind: 'item' | 'jewel' };
  actionClassName?: 'trade-primary' | 'trade-secondary';
  damage?: DamageHeadline;
}) {
  const ut = (key: Parameters<typeof upgradeT>[1]) => upgradeT(lang, key);
  const goTo = useNavigateToPage();
  const defaultDamage = useHeadlineDamage(session.calc, lang);
  const damage = providedDamage ?? defaultDamage;
  const [saved, setSaved] = useState(false);
  const [text, setText] = useState(initialItemText ?? '');
  const [snapshot, setSnapshot] = useState<ComparisonSnapshot | null>(null);
  const [chosenSlot, setChosenSlot] = useState<string | null>(null);
  const [receipt, setReceipt] = useState<AppliedReplacement | null>(null);
  // An edit invalidates the comparison, but a verified install belongs to the new revision.
  const applied = receipt && receipt.scope === session.calculationScope && receipt.version === session.stateVersion
    && receipt.weaponSet === session.activeWeaponSet && receipt.installed(session) ? receipt : null;
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
  const appliedRef = useRef<HTMLParagraphElement>(null);
  const focusedReceipt = useRef<AppliedReplacement | null>(null);
  // Only allocated sockets can be replacement destinations. Loading other tree sockets must not invalidate an item comparison.
  const allocatedJewelSockets = useMemo(() => jewelSockets.filter(id => session.allocatedNodes.includes(id)), [jewelSockets, session.allocatedNodes]);
  const contextKey = useMemo(() => JSON.stringify([
    session.stateVersion, session.activeWeaponSet, session.currentRequest(), objective, session.dataVersion, session.treeVersion, allocatedJewelSockets,
  ]), [session.stateVersion, session.currentRequest, session.activeWeaponSet, objective, session.dataVersion, session.treeVersion, allocatedJewelSockets]);
  const key = useMemo(() => JSON.stringify([contextKey, text, augmentPlans]), [contextKey, text, augmentPlans]);
  const latest = useRef({ active, contextKey, key, catalog, applied, session });
  latest.current = { active, contextKey, key, catalog, applied, session };
  const previousContext = useRef({ key: contextKey, catalog });
  // Hide incompatible comparisons before effects run, including while this page is hidden.
  const report = snapshot?.key === key && snapshot.catalog === catalog ? snapshot.report : null;
  const preview = usePreview();
  const positions = report ? [...report.positions].sort((a, b) => compareObjectiveStats(a.stats, b.stats, objective) || a.slot.localeCompare(b.slot)) : [];
  const selected = positions.find(row => row.slot === chosenSlot) ?? positions[0];
  const best = positions[0];
  const recommended = report && best && !best.unsupported.length && !best.augments.limitWarning && feasibleOf(best.stats, objective)
    && compareObjectiveStats(best.stats, report.baseline, objective) < 0 ? best.slot : undefined;
  const affixes = replacementAffixes(selected?.lines ?? report?.lines ?? []);
  const originalName = report?.lines.find(line => line.kind === 'name')?.text ?? '';
  const baseNames = useLocalizedNames(report ? [{ kind: 'base', name: report.base.name }] : [], lang);
  const name = useLocalizedLines([originalName], lang);
  const baseName = report ? itemNameLabel(report.base.name, baseNames[0], lang) : '';
  const translated = useLocalizedLines(['', ...affixes.map(line => line.text)], lang);
  const nameLine = originalName && originalName !== report?.base.name ? itemNameLabel(originalName, name[0], lang) : baseName;
  const requiredLevel = Math.max(report?.requiredLevel ?? 0, ...(selected?.augments.runes.map(name =>
    selected.augments.info.options.find(option => option.name === name)?.required_level ?? 0) ?? []));
  const selectedItem = selected?.variant.set_items?.length === 1 ? selected.variant.set_items[0] : undefined;
  const alreadyAdded = !!report && !!selectedItem && equipmentCandidates.some(candidate =>
    candidate.itemId === report.text && candidate.slot === selectedItem.slot && candidate.text === selectedItem.text);
  const deltaText = (value: number) => formatSignedNumber(value);
  const positionLabel = (slot: string) => slot.startsWith('Jewel@') ? socketLabel?.(Number(slot.slice(6))) ?? ut('jewelPosition') : slotLabel(lang, slot);
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
  // Trade temporarily becomes inactive while its baseline recalculates. That
  // cancels comparison work, not the receipt for the edit just installed.
  useLayoutEffect(() => { if (!active) stop(); }, [active, stop]);
  useLayoutEffect(() => {
    if (receipt && session.stateVersion < receipt.version && !session.busy) setReceipt(null);
  }, [receipt, session.stateVersion, session.busy]);
  useLayoutEffect(() => {
    const target = appliedRef.current;
    if (!active || !applied || !target || focusedReceipt.current === applied) return;
    focusedReceipt.current = applied;
    if (document.activeElement !== document.body) return; // The user moved on while recalculating.
    target.focus({ preventScroll: true });
    target.scrollIntoView({ block: 'nearest', behavior: 'instant' });
  }, [active, applied]);
  useEffect(() => { if (report && inputRef.current) inputRef.current.scrollTop = 0; }, [report?.text]);
  useLayoutEffect(() => () => stop(), [stop]);

  const run = useCallback(async (input: string, plans: Record<string, AugmentSelection> = {}, keepSlot?: string) => {
    const request = session.currentRequest();
    const context = JSON.stringify([session.stateVersion, session.activeWeaponSet, request, objective, session.dataVersion, session.treeVersion, allocatedJewelSockets]);
    if (!active || !latest.current.active || session.busy || !request || !catalog
      || latest.current.contextKey !== context || latest.current.catalog !== catalog) return;
    stop();
    setReceipt(null); setSaved(false);
    const abort = new AbortController(); controller.current = abort;
    const nextKey = JSON.stringify([context, input, plans]);
    const current = () => !abort.signal.aborted && controller.current === abort && latest.current.active
      && latest.current.key === nextKey && latest.current.catalog === catalog;
    setBusy(true); setError(''); setAugmentPlans(plans);
    if (!keepSlot) { setSnapshot(null); setChosenSlot(null); setAffixDirty(false); }
    else setChosenSlot(keepSlot);
    try {
      const next = await compareReplacementPositions(request, input, abort.signal, { catalog, jewelSocketNodes: allocatedJewelSockets, augmentPlans: plans, includeEmptySlots: !!candidate });
      if (current()) setSnapshot({ report: next, key: nextKey, request, input, objective, catalog,
        dataVersion: session.dataVersion, treeVersion: session.treeVersion, augmentPlans: plans });
    } catch (error) {
      if (current()) { setSnapshot(null); setAffixDirty(false); setError(error instanceof Error ? error.message : 'invalid-item'); }
    } finally {
      if (controller.current === abort) { setBusy(false); controller.current = null; }
    }
  }, [active, session.stateVersion, session.currentRequest, session.activeWeaponSet, session.dataVersion, session.treeVersion,
    session.busy, objective, catalog, allocatedJewelSockets, stop, candidate?.kind]);
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
    && snapshot.key === JSON.stringify([JSON.stringify([session.stateVersion, session.activeWeaponSet, session.currentRequest(), objective,
      session.dataVersion, session.treeVersion, allocatedJewelSockets]), text, augmentPlans]);

  const apply = () => {
    if (!active || !latest.current.active || latest.current.session !== session
      || busy || affixDirty || session.busy || !report || !selected || !receiptCurrent()) return;
    const { jewels, flasks, set_items: replacements } = selected.variant;
    const jewel = jewels?.find(row => `Jewel@${row.socket_node}` === selected.slot);
    const flask = flasks?.find(row => row.slot === selected.slot);
    const item = replacements?.find(row => row.slot === selected.slot);
    // Check the exact payload passed to the session, not the pasted text. Rune
    // inheritance may rewrite it; a rejected async jewel preflight installs nothing.
    let installed: AppliedReplacement['installed'];
    let installedText: string;
    if (jewels && jewel) {
      session.setJewels(jewels);
      installed = current => current.jewels === jewels;
      installedText = jewel.text;
    } else if (flasks && flask) {
      session.setFlasks(flasks);
      installed = current => current.flasks === flasks;
      installedText = flask.text;
    } else if (replacements && item) {
      session.setItems([...session.items.filter(row => !replacements.some(replacement => replacement.slot === row.slot)), ...replacements]);
      installed = current => replacements.every(row => current.items.includes(row));
      installedText = item.text;
    } else return;
    latest.current.key = '';
    setReceipt({ scope: session.calculationScope, version: session.stateVersion + 1, annotations: session.annotations,
      slot: selected.slot, name: nameLine || baseName, text: installedText,
      weaponSet: session.activeWeaponSet, installed });
    stop(); setSnapshot(null); setChosenSlot(null); setAffixDirty(false);
  };
  const undo = () => {
    const current = latest.current;
    // Session undo restores the whole state; annotations do not advance its calculation version.
    if (!applied || current.applied !== applied || !current.active || current.session.busy
      || !current.session.canUndo || current.session.annotations !== applied.annotations) return;
    current.applied = null;
    current.session.undoEdit();
    setReceipt(null);
    inputRef.current?.focus({ preventScroll: true });
    inputRef.current?.scrollIntoView({ block: 'nearest', behavior: 'instant' });
  };
  const addCandidate = () => {
    if (!active || !latest.current.active || busy || affixDirty || session.busy || !report || !selectedItem || !receiptCurrent() || alreadyAdded
      || equipmentCandidates.length >= EQUIPMENT_CANDIDATE_LIMIT) return;
    const candidate = { id: String(++candidateSequence.current), itemId: report.text,
      label: nameLine || baseName, slot: selectedItem.slot, text: selectedItem.text };
    setEquipmentCandidates(current => [...current, candidate]);
  };
  const candidateText = selected?.variant.set_items?.find(item => item.slot === selected.slot)?.text
    ?? selected?.variant.flasks?.find(item => item.slot === selected.slot)?.text
    ?? selected?.variant.jewels?.find(jewel => `Jewel@${jewel.socket_node}` === selected.slot)?.text ?? applied?.text ?? text;
  return <section className="upgrade-item-check ui-card" aria-labelledby="replacement-heading">
    {candidate ? <header><h3 id="replacement-heading">{craftT(lang, 'preview')}</h3><p>{craftT(lang, 'previewHint')}</p></header> : <header className="replacement-heading"><div>
      <h3 id="replacement-heading">{ut('pasteAction')}</h3><p>{ut('checkHint')}</p></div><div className="replacement-context">
      <AppButton className="replacement-goal" onClick={onEditGoal}>{t(lang, 'format.parenthetical', { label: `${ut('comparisonGoal')}${colon(lang)}${goalLabel}`, detail: ut('changeGoal') })}</AppButton></div></header>}
    <div className="replacement-input"><label htmlFor="replacement-text">{ut('itemText')}</label>
      <AppTextarea readOnly={!!candidate} ref={inputRef} id="replacement-text" rows={report ? 3 : 5} value={text} placeholder={ut('pastePlaceholder')}
        onChange={event => { if (candidate || !latest.current.active) return; stop(); setReceipt(null); setSnapshot(null); setChosenSlot(null); setAffixDirty(false); setAugmentPlans({}); setText(event.target.value); }}
        onPaste={event => { if (candidate || !latest.current.active) return; const copied = event.clipboardData.getData('text/plain'); if (!copied) return;
          event.preventDefault(); stop(); setReceipt(null); setSnapshot(null); setAugmentPlans({}); setChosenSlot(null); setAffixDirty(false); setText(copied); setPendingPaste({ input: copied }); }} />
      <div className="replacement-actions"><AppButton className={`replacement-compare ${actionClassName}`} disabled={!active || busy || affixDirty || session.busy || !catalog || !text.trim()} onClick={() => void run(text, augmentPlans, chosenSlot ?? undefined)}>
        {busy ? ut('comparingPositions') : candidate ? craftT(lang, 'compareEquipped') : ut('compare')}</AppButton>
        {busy && <AppButton onClick={stop}>{ut('cancelCompare')}</AppButton>}</div>
      {!candidate && <p className="replacement-copy-links"><a href="/userscripts/pobr-market-copy.user.js" target="_blank" rel="noreferrer">{ut('copyScript')}</a>
        <a href="/userscripts/index.html" target="_blank" rel="noreferrer">{ut('copyScriptHelp')}</a></p>}
    </div>
    {candidate && <div className="replacement-actions"><AppButton disabled={busy || session.busy || affixDirty || saved || !active} onClick={() => {
      session.saveLibraryItem(candidate.kind, candidateText, selected?.slot.startsWith('Jewel@') ? undefined : selected?.slot); setSaved(true);
    }}>{craftT(lang, saved ? 'saved' : 'save')}</AppButton></div>}
    {error && <div className="trade-notice trade-error">
      <p role="alert">{errorText(error)}</p>
      {!candidate && error === 'no-compatible-slot' && <div className="replacement-actions">
        <AppButton onClick={() => goTo('items')}>{ut('emptyEquipment')}</AppButton>
        <AppButton onClick={() => goTo('tree')}>{ut('allocateSocket')}</AppButton>
      </div>}
    </div>}
    {active && applied && <div className="trade-notice replacement-applied">
      <p role="status" tabIndex={-1} ref={appliedRef}>
        {ut('applied').replace(/\{(item|slot)\}/g, (_, key) => key === 'item' ? applied.name : positionLabel(applied.slot))}
      </p>
      {session.canUndo && applied.annotations === session.annotations
        && <AppButton disabled={session.busy} onClick={undo}>{t(lang, 'shell.undo')}</AppButton>}
    </div>}
    {report && selected && <div className="replacement-content" aria-busy={busy}>
      <div className="replacement-candidate">
      <aside className="replacement-parsed" aria-label={ut('parsedItem')}>
        <span className="trade-section-label">{ut('parsedItem')}</span><h4>{nameLine || baseName}</h4>
        <p>{baseName} · {ut('requiredLevel')} {requiredLevel}</p>
        <div className="replacement-affixes"><span className="replacement-affix-count">{ut('parsedAffixes')} · {affixes.length}</span>
          {affixes.map((line, index) => isContinuation(line.text, translated[index + 1]) ? null : <div key={index} className={`replacement-affix replacement-affix--${line.kind}`}>
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
            onMouseEnter={preview ? event => preview.showChanges(`${positionLabel(row.slot)} ← ${ut('replacementOutcome')}`, changesFromRecords(report.baseline, row.stats), event.currentTarget) : undefined}
            onMouseLeave={preview?.clear} onFocus={preview ? event => preview.showChanges(`${positionLabel(row.slot)} ← ${ut('replacementOutcome')}`, changesFromRecords(report.baseline, row.stats), event.currentTarget) : undefined} onBlur={preview?.clear}>
            <strong>{positionLabel(row.slot)}</strong>{recommended === row.slot && <span className="replacement-recommended">{ut('recommendedPosition')}</span>}
            <span className={(row.stats[damage.stat] ?? 0) < (report.baseline[damage.stat] ?? 0) ? 'delta-neg' : ''}>{damage.label} {deltaText((row.stats[damage.stat] ?? 0) - (report.baseline[damage.stat] ?? 0))}</span>
            <span className={(row.stats.TotalEHP ?? 0) < (report.baseline.TotalEHP ?? 0) ? 'delta-neg' : ''}>{statNameLabel(lang, 'TotalEHP')} {deltaText((row.stats.TotalEHP ?? 0) - (report.baseline.TotalEHP ?? 0))}</span>
          </AppButton>)}
        </div>
        <div className="replacement-verdict"><span className="trade-section-label">{ut('replacementOutcome')} · {positionLabel(selected.slot)}</span>
          <strong className={selected.unsupported.length || selected.augments.limitWarning ? '' : compareObjectiveStats(selected.stats, report.baseline, objective) < 0 ? 'delta-pos' : 'delta-neg'}>
            {selected.augments.limitWarning ? ut('augmentUnverified') : selected.unsupported.length ? ut('partialReplacement') : ut(compareObjectiveStats(selected.stats, report.baseline, objective) < 0 ? 'better' : 'worse')}</strong>
          {(selected.stats[damage.stat] ?? 0) < (report.baseline[damage.stat] ?? 0) && <p className="delta-neg">{ut('damageDecrease').replace('{stat}', () => damage.label)}</p>}
          {objective.constraints.length > 0 && <ul className="replacement-constraints" aria-label={ut('statLimits')}>
            {objective.constraints.map((constraint, index) => <li key={`${constraint.stat}:${index}`}>
              {statNameLabel(lang, constraint.stat)} {[
                constraint.min === undefined ? '' : `≥ ${formatStat(constraint.stat, constraint.min, lang)}`,
                constraint.max === undefined ? '' : `≤ ${formatStat(constraint.stat, constraint.max, lang)}`,
              ].filter(Boolean).join(' · ')}
            </li>)}
          </ul>}
          {!feasibleOf(selected.stats, objective) && <p className="delta-neg">{ut('constraintFail')}</p>}
          {!!objective.softMinimums?.length && <p className="replacement-resistance">{ut('resistancePriority').replace('{target}', () => formatNumber(objective.softMinimums![0].min))} · {ut('resistanceGap')}{colon(lang)}{formatNumber(minimumDeficit(report.baseline, objective))} → {formatNumber(minimumDeficit(selected.stats, objective))}</p>}
        </div>
        <SwapGainCard before={Object.entries(report.baseline).map(([id, value]) => ({ id, value }))}
          after={Object.entries(selected.stats).map(([id, value]) => ({ id, value }))} lang={lang} damage={damage} />
        {!!selected.unsupported.length && <details className="trade-notice replacement-unsupported"><summary className="disclosure">{ut('incomplete')} ({selected.unsupported.length})</summary><UnincludedModifiers lines={selected.unsupported} lang={lang} /></details>}
        {!!report.rejected.length && <p className="trade-notice">{report.rejected.map(row => `${positionLabel(row.slot)}: ${errorText(row.reason)}`).join('\n')}</p>}
        <div className="replacement-apply"><AppButton className={actionClassName} disabled={!active || busy || affixDirty || session.busy} onClick={apply}>{busy ? ut('comparingPositions') : `${ut('apply')} · ${positionLabel(selected.slot)}`}</AppButton><span>{affixDirty ? affixT(lang, 'dirty') : ut('applyHint')}</span></div>
        {!candidate && selectedItem && <div className="replacement-actions"><AppButton disabled={!active || busy || affixDirty || session.busy || alreadyAdded || equipmentCandidates.length >= EQUIPMENT_CANDIDATE_LIMIT}
          onClick={addCandidate}>{ut(alreadyAdded ? 'jointAdded' : 'jointAdd')}</AppButton>
          <span>{equipmentCandidates.length} / {EQUIPMENT_CANDIDATE_LIMIT}</span></div>}
      </div>
    </div>}
    {!candidate && <details className="equipment-fold" open={equipmentCandidates.length > 0}>
      <summary className="disclosure" data-disclosure-level="section"><span>{ut('jointEquipment')}</span><small>{equipmentCandidates.length} / {EQUIPMENT_CANDIDATE_LIMIT}</small></summary>
      <EquipmentPlanner session={session} lang={lang} active={active} objective={objective} catalog={catalog} candidates={equipmentCandidates} actionClassName={actionClassName} damage={damage}
        onRemove={id => setEquipmentCandidates(current => current.filter(candidate => candidate.id !== id))} /></details>}
  </section>;
}
