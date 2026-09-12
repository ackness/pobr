import { useCallback, useEffect, useRef, useState } from 'react';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useLocalizedLines } from '../../hooks/useLocalizedLines';
import { compareReplacementPositions, replacementAffixes, type ReplacementReport } from '../../lib/itemReplacement';
import { compareObjectiveStats, feasibleOf, type Objective } from '../../lib/optimize';
import type { TradeCatalog } from '../../lib/tradeOptimizer';
import { slotLabel, statNameLabel, type Lang } from '../../lib/i18n';
import { upgradeT } from '../../lib/upgradeText';
import type { AugmentSelection } from '../../lib/replacementAugments';
import { ReplacementAugmentPicker } from './ReplacementAugmentPicker';

interface ComparisonSnapshot { report: ReplacementReport; requestKey: string; input: string }

/** One visible clipboard flow; choosing a destination never mutates the build. */
export function ItemReplacementCheck({ session, lang, objective, catalog, jewelSockets, goalLabel, onEditGoal, initialItemText }: {
  session: BuildSession; lang: Lang; objective: Objective; catalog: TradeCatalog | null; jewelSockets: number[];
  goalLabel: string; onEditGoal: () => void;
  initialItemText?: string;
}) {
  const ut = (key: Parameters<typeof upgradeT>[1]) => upgradeT(lang, key);
  const [text, setText] = useState(initialItemText ?? '');
  const [snapshot, setSnapshot] = useState<ComparisonSnapshot | null>(null);
  const [chosenSlot, setChosenSlot] = useState<string | null>(null);
  const [pendingPaste, setPendingPaste] = useState<string | null>(initialItemText ?? null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [augmentPlans, setAugmentPlans] = useState<Record<string, AugmentSelection>>({});
  const controller = useRef<AbortController | null>(null);
  const inputRef = useRef<HTMLTextAreaElement | null>(null);
  const requestKey = JSON.stringify([session.activeWeaponSet, session.currentRequest()]);
  // Hide stale results during render, before the invalidation effect runs.
  const report = snapshot?.requestKey === requestKey && snapshot.input === text ? snapshot.report : null;
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
  const format = (value: number) => value.toLocaleString(lang, { maximumFractionDigits: 2 });
  const deltaText = (value: number) => `${value > 0 ? '+' : ''}${format(value)}`;
  const positionLabel = (slot: string) => slot.startsWith('Jewel@') ? `${ut('jewelPosition')} ${slot.slice(6)}` : slotLabel(lang, slot);
  const errorText = useCallback((message: string) => ({
    'invalid-item': upgradeT(lang, 'invalidItem'), 'unknown-base': upgradeT(lang, 'unknownBase'),
    'wrong-slot': upgradeT(lang, 'wrongSlot'), 'weapon-type': upgradeT(lang, 'weaponType'),
    'item-level': upgradeT(lang, 'itemLevel'), 'unidentified-item': upgradeT(lang, 'unidentified'),
    'no-compatible-slot': upgradeT(lang, 'noCompatibleSlot'),
    'invalid-augments': upgradeT(lang, 'augmentInvalid'),
    'augment-limit': upgradeT(lang, 'augmentLimit'),
  }[message] ?? message), [lang]);
  useEffect(() => {
    controller.current?.abort(); setSnapshot(null); setChosenSlot(null); setError(''); setBusy(false); setAugmentPlans({});
  }, [session.currentRequest, text]);
  useEffect(() => { setChosenSlot(null); }, [objective]);
  useEffect(() => { if (report && inputRef.current) inputRef.current.scrollTop = 0; }, [report?.text]);
  useEffect(() => () => controller.current?.abort(), []);

  const run = useCallback(async (input: string, plans: Record<string, AugmentSelection> = {}, keepSlot?: string) => {
    const request = session.currentRequest();
    if (!request || !catalog) return;
    controller.current?.abort();
    const abort = new AbortController(); controller.current = abort;
    const key = JSON.stringify([session.activeWeaponSet, request]);
    setPendingPaste(null); setBusy(true); setError('');
    if (!keepSlot) { setSnapshot(null); setChosenSlot(null); }
    try {
      const next = await compareReplacementPositions(request, input, abort.signal, { catalog, jewelSocketNodes: jewelSockets, augmentPlans: plans });
      if (!abort.signal.aborted) setSnapshot({ report: next, requestKey: key, input });
    } catch (error) {
      if (!abort.signal.aborted) { setSnapshot(null); setError(errorText(error instanceof Error ? error.message : 'invalid-item')); }
    } finally {
      if (controller.current === abort) { setBusy(false); controller.current = null; }
    }
  }, [session.currentRequest, session.activeWeaponSet, catalog, jewelSockets, errorText]);
  // Read only an explicit paste event. Manual edits retain the Calculate action.
  useEffect(() => {
    if (pendingPaste === null || session.busy || !catalog) return;
    const timer = setTimeout(() => void run(pendingPaste), 150);
    return () => clearTimeout(timer);
  }, [pendingPaste, session.busy, catalog, run]);

  const apply = () => {
    if (busy || !report || !selected || snapshot?.requestKey !== JSON.stringify([session.activeWeaponSet, session.currentRequest()])) return;
    // Apply the evaluated per-position payload, preserving all unrelated sources.
    if (selected.variant.jewels) session.setJewels(selected.variant.jewels);
    else if (selected.variant.flasks) session.setFlasks(selected.variant.flasks);
    else if (selected.variant.set_items) {
      const replacements = selected.variant.set_items;
      session.setItems([...session.items.filter(item => !replacements.some(row => row.slot === item.slot)), ...replacements]);
    }
    setSnapshot(null); setChosenSlot(null);
  };
  return <section className="upgrade-item-check ui-card" aria-labelledby="replacement-heading">
    <header className="replacement-heading"><div><span className="trade-section-label">{ut('pasteEyebrow')}</span>
      <h3 id="replacement-heading">{ut('check')}</h3><p>{ut('checkHint')}</p></div><div className="replacement-context"><span className="ui-badge">{ut('localComparison')}</span>
      <button className="replacement-goal" aria-label={`${ut('comparisonGoal')}: ${goalLabel}`} onClick={onEditGoal}>{goalLabel} <span aria-hidden>↗</span></button></div></header>
    <div className="replacement-input"><label htmlFor="replacement-text">{ut('itemText')}</label>
      <textarea ref={inputRef} id="replacement-text" rows={report ? 3 : 5} value={text} placeholder={ut('pastePlaceholder')}
        onChange={event => { setPendingPaste(null); setText(event.target.value); }}
        onPaste={event => { const copied = event.clipboardData.getData('text/plain'); if (!copied) return;
          event.preventDefault(); controller.current?.abort(); setSnapshot(null); setAugmentPlans({}); setChosenSlot(null); setText(copied); setPendingPaste(copied); }} />
      <div className="replacement-actions"><button className="trade-primary" disabled={busy || session.busy || !catalog || !text.trim()} onClick={() => void run(text, augmentPlans)}>
        {busy ? ut('comparingPositions') : ut('compare')}</button>
        {busy && <button onClick={() => { controller.current?.abort(); controller.current = null; setPendingPaste(null); setSnapshot(null); setBusy(false); }}>{ut('cancelCompare')}</button>}
        <span>{ut('autoPositions')}</span></div>
      <p className="replacement-copy-links"><a href="/userscripts/pobr-market-copy.user.js" target="_blank" rel="noreferrer">{ut('copyScript')}</a>
        <a href="/userscripts/index.html" target="_blank" rel="noreferrer">{ut('copyScriptHelp')}</a></p>
    </div>
    {error && <p role="alert" className="trade-error">{error}</p>}
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
      <ReplacementAugmentPicker key={selected.slot} plan={selected.augments} level={session.currentRequest()?.character?.level ?? 1} lang={lang} disabled={busy || session.busy}
        onChange={selection => { const plans = { ...augmentPlans, [selected.slot]: selection }; setAugmentPlans(plans); setChosenSlot(selected.slot); void run(text, plans, selected.slot); }} />
      </div>
      <div className="upgrade-replacement-result" aria-live="polite">
        <div className="replacement-destinations" role="group" aria-label={ut('chooseReplacement')}>
          {positions.map(row => <button key={row.slot} disabled={busy} className="replacement-position" aria-pressed={selected.slot === row.slot} onClick={() => setChosenSlot(row.slot)}>
            <strong>{positionLabel(row.slot)}</strong>{recommended === row.slot && <span className="replacement-recommended">{ut('recommendedPosition')}</span>}
            <span className={(row.stats.TotalDPS ?? 0) < (report.baseline.TotalDPS ?? 0) ? 'delta-neg' : ''}>DPS {deltaText((row.stats.TotalDPS ?? 0) - (report.baseline.TotalDPS ?? 0))}</span>
            <span className={(row.stats.TotalEHP ?? 0) < (report.baseline.TotalEHP ?? 0) ? 'delta-neg' : ''}>EHP {deltaText((row.stats.TotalEHP ?? 0) - (report.baseline.TotalEHP ?? 0))}</span>
          </button>)}
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
        <div className="replacement-apply"><button className="trade-primary" disabled={busy || session.busy} onClick={apply}>{busy ? ut('comparingPositions') : `${ut('apply')} · ${positionLabel(selected.slot)}`}</button><span>{ut('applyHint')}</span></div>
      </div>
    </div>}
  </section>;
}
