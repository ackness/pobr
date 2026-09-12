import { useCallback, useEffect, useRef, useState } from 'react';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useLocalizedLines } from '../../hooks/useLocalizedLines';
import { compareReplacementPositions, replacementAffixes, type ReplacementReport } from '../../lib/itemReplacement';
import { compareObjectiveStats, feasibleOf, type Objective } from '../../lib/optimize';
import type { TradeCatalog } from '../../lib/tradeOptimizer';
import { slotLabel, statNameLabel, type Lang } from '../../lib/i18n';
import { upgradeT } from '../../lib/upgradeText';

interface ComparisonSnapshot { report: ReplacementReport; requestKey: string; input: string }

/** One visible clipboard flow; choosing a destination never mutates the build. */
export function ItemReplacementCheck({ session, lang, objective, catalog, jewelSockets, goalLabel, onEditGoal }: {
  session: BuildSession; lang: Lang; objective: Objective; catalog: TradeCatalog | null; jewelSockets: number[];
  goalLabel: string; onEditGoal: () => void;
}) {
  const ut = (key: Parameters<typeof upgradeT>[1]) => upgradeT(lang, key);
  const [text, setText] = useState('');
  const [snapshot, setSnapshot] = useState<ComparisonSnapshot | null>(null);
  const [chosenSlot, setChosenSlot] = useState<string | null>(null);
  const [pendingPaste, setPendingPaste] = useState<string | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const controller = useRef<AbortController | null>(null);
  const requestKey = JSON.stringify([session.activeWeaponSet, session.currentRequest()]);
  // Hide stale results during render, before the invalidation effect runs.
  const report = snapshot?.requestKey === requestKey && snapshot.input === text ? snapshot.report : null;
  const positions = report ? [...report.positions].sort((a, b) => compareObjectiveStats(a.stats, b.stats, objective) || a.slot.localeCompare(b.slot)) : [];
  const selected = positions.find(row => row.slot === chosenSlot) ?? positions[0];
  const best = positions[0];
  const recommended = report && best && !best.unsupported.length && feasibleOf(best.stats, objective)
    && compareObjectiveStats(best.stats, report.baseline, objective) < 0 ? best.slot : undefined;
  const affixes = replacementAffixes(report?.lines ?? []);
  const translated = useLocalizedLines([report?.base.name ?? '', ...affixes.map(line => line.text)], lang);
  const nameLine = report?.lines.find(line => line.kind === 'name')?.text;
  const format = (value: number) => value.toLocaleString(lang, { maximumFractionDigits: 2 });
  const deltaText = (value: number) => `${value > 0 ? '+' : ''}${format(value)}`;
  const positionLabel = (slot: string) => slot.startsWith('Jewel@') ? `${ut('jewelPosition')} ${slot.slice(6)}` : slotLabel(lang, slot);
  const errorText = useCallback((message: string) => ({
    'invalid-item': upgradeT(lang, 'invalidItem'), 'unknown-base': upgradeT(lang, 'unknownBase'),
    'wrong-slot': upgradeT(lang, 'wrongSlot'), 'weapon-type': upgradeT(lang, 'weaponType'),
    'item-level': upgradeT(lang, 'itemLevel'), 'unidentified-item': upgradeT(lang, 'unidentified'),
    'no-compatible-slot': upgradeT(lang, 'noCompatibleSlot'),
  }[message] ?? message), [lang]);
  useEffect(() => {
    controller.current?.abort(); setSnapshot(null); setChosenSlot(null); setError(''); setBusy(false);
  }, [session.currentRequest, text]);
  useEffect(() => { setChosenSlot(null); }, [objective]);
  useEffect(() => () => controller.current?.abort(), []);

  const run = useCallback(async (input: string) => {
    const request = session.currentRequest();
    if (!request || !catalog) return;
    controller.current?.abort();
    const abort = new AbortController(); controller.current = abort;
    const key = JSON.stringify([session.activeWeaponSet, request]);
    setPendingPaste(null); setBusy(true); setError(''); setSnapshot(null); setChosenSlot(null);
    try {
      const next = await compareReplacementPositions(request, input, abort.signal, { catalog, jewelSocketNodes: jewelSockets });
      if (!abort.signal.aborted) setSnapshot({ report: next, requestKey: key, input });
    } catch (error) {
      if (!abort.signal.aborted) setError(errorText(error instanceof Error ? error.message : 'invalid-item'));
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
    if (!report || !selected || snapshot?.requestKey !== JSON.stringify([session.activeWeaponSet, session.currentRequest()])) return;
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
      <h3 id="replacement-heading">{ut('check')}</h3><p>{ut('checkHint')}</p></div><span className="ui-badge">{ut('localComparison')}</span></header>
    <button className="replacement-goal" onClick={onEditGoal}>{ut('comparisonGoal')}: {goalLabel} <span aria-hidden>↗</span></button>
    <div className="replacement-input"><label htmlFor="replacement-text">{ut('itemText')}</label>
      <textarea id="replacement-text" rows={5} value={text} placeholder={ut('pastePlaceholder')}
        onChange={event => { setPendingPaste(null); setText(event.target.value); }}
        onPaste={event => { const copied = event.clipboardData.getData('text/plain'); if (!copied) return;
          event.preventDefault(); setText(copied); setPendingPaste(copied); }} />
      <div className="replacement-actions"><button className="trade-primary" disabled={busy || session.busy || !catalog || !text.trim()} onClick={() => void run(text)}>
        {busy ? ut('comparingPositions') : ut('compare')}</button>
        {busy && <button onClick={() => { controller.current?.abort(); controller.current = null; setPendingPaste(null); setBusy(false); }}>{ut('cancelCompare')}</button>}
        <span>{ut('autoPositions')}</span></div>
      <p className="replacement-copy-links"><a href="/userscripts/pobr-market-copy.user.js" target="_blank" rel="noreferrer">{ut('copyScript')}</a>
        <a href="/userscripts/index.html" target="_blank" rel="noreferrer">{ut('copyScriptHelp')}</a></p>
    </div>
    {error && <p role="alert" className="trade-error">{error}</p>}
    {report && selected && <div className="replacement-content">
      <aside className="replacement-parsed" aria-label={ut('parsedItem')}>
        <span className="trade-section-label">{ut('parsedItem')}</span><h4>{nameLine || translated[0]}</h4>
        <p>{translated[0]} · {ut('requiredLevel')} {report.requiredLevel}</p>
        <div className="replacement-affixes"><span className="replacement-affix-count">{ut('parsedAffixes')} · {affixes.length}</span>
          {affixes.map((line, index) => <div key={index} className={`replacement-affix replacement-affix--${line.kind}`}>
            <span>{translated[index + 1] ?? line.text}</span>{line.tier !== undefined && <small>T{line.tier}</small>}
          </div>)}{!affixes.length && <p>{ut('noAffixes')}</p>}
        </div>
        {report.lines.some(line => line.kind === 'class_req') && <p className="trade-notice">{report.lines.filter(line => line.kind === 'class_req').map(line => line.text).join('\n')}</p>}
        <p className="replacement-requirement-note">{ut('requirementsNote')}</p>
      </aside>
      <div className="upgrade-replacement-result" aria-live="polite">
        <div className="replacement-destinations" role="group" aria-label={ut('chooseReplacement')}>
          {positions.map(row => <button key={row.slot} className="replacement-position" aria-pressed={selected.slot === row.slot} onClick={() => setChosenSlot(row.slot)}>
            <strong>{positionLabel(row.slot)}</strong>{recommended === row.slot && <span className="replacement-recommended">{ut('recommendedPosition')}</span>}
            <span className={(row.stats.TotalDPS ?? 0) < (report.baseline.TotalDPS ?? 0) ? 'delta-neg' : ''}>DPS {deltaText((row.stats.TotalDPS ?? 0) - (report.baseline.TotalDPS ?? 0))}</span>
            <span className={(row.stats.TotalEHP ?? 0) < (report.baseline.TotalEHP ?? 0) ? 'delta-neg' : ''}>EHP {deltaText((row.stats.TotalEHP ?? 0) - (report.baseline.TotalEHP ?? 0))}</span>
          </button>)}
        </div>
        <strong className={selected.unsupported.length ? '' : compareObjectiveStats(selected.stats, report.baseline, objective) < 0 ? 'delta-pos' : 'delta-neg'}>
          {selected.unsupported.length ? ut('partialReplacement') : ut(compareObjectiveStats(selected.stats, report.baseline, objective) < 0 ? 'better' : 'worse')}</strong>
        {(selected.stats.TotalDPS ?? 0) < (report.baseline.TotalDPS ?? 0) && <p className="delta-neg">{ut('dpsDrop')}</p>}
        {!feasibleOf(selected.stats, objective) && <p className="delta-neg">{ut('constraintFail')}</p>}
        <div className="replacement-table-wrap"><table><thead><tr><th>{positionLabel(selected.slot)}</th><th>{ut('before')}</th><th>{ut('after')}</th><th>{ut('change')}</th></tr></thead><tbody>
          {['TotalDPS', 'TotalEHP', 'Life', 'FireResist', 'ColdResist', 'LightningResist', 'ChaosResist'].map(stat => {
            const before = report.baseline[stat] ?? 0, after = selected.stats[stat] ?? 0, delta = after - before;
            return <tr key={stat}><th>{statNameLabel(lang, stat)}</th><td>{format(before)}</td><td>{format(after)}</td><td className={delta < 0 ? 'delta-neg' : delta > 0 ? 'delta-pos' : ''}>{deltaText(delta)}</td></tr>;
          })}</tbody></table></div>
        {!!selected.unsupported.length && <details open className="trade-notice replacement-unsupported"><summary>{ut('incomplete')}</summary><pre>{selected.unsupported.join('\n')}</pre></details>}
        {!!report.rejected.length && <p className="trade-notice">{report.rejected.map(row => `${positionLabel(row.slot)}: ${errorText(row.reason)}`).join('\n')}</p>}
        <div className="replacement-apply"><button className="trade-primary" disabled={session.busy} onClick={apply}>{ut('apply')} · {positionLabel(selected.slot)}</button><span>{ut('applyHint')}</span></div>
      </div>
    </div>}
  </section>;
}
