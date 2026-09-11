import { useEffect, useRef, useState } from 'react';
import type { BuildSession } from '../../hooks/useBuildSession';
import { compareReplacement } from '../../lib/itemReplacement';
import { compareObjectiveStats, feasibleOf, type Objective } from '../../lib/optimize';
import { statNameLabel, type Lang } from '../../lib/i18n';
import { upgradeT } from '../../lib/upgradeText';

export function ItemReplacementCheck({ session, slot, lang, objective }: {
  session: BuildSession; slot: string; lang: Lang; objective: Objective;
}) {
  const ut = (key: Parameters<typeof upgradeT>[1]) => upgradeT(lang, key);
  const [text, setText] = useState('');
  const [result, setResult] = useState<Awaited<ReturnType<typeof compareReplacement>> | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const controller = useRef<AbortController | null>(null);
  useEffect(() => {
    controller.current?.abort(); setResult(null); setError(''); setBusy(false);
  }, [session.currentRequest, slot, text]);
  useEffect(() => () => controller.current?.abort(), []);
  const run = async () => {
    const request = session.currentRequest();
    if (!request) return;
    const abort = new AbortController(); controller.current = abort;
    setBusy(true); setError(''); setResult(null);
    try { const next = await compareReplacement(request, slot, text, abort.signal); if (!abort.signal.aborted) setResult(next); }
    catch (error) { if (!abort.signal.aborted) setError(error instanceof Error ? ({ 'invalid-item': ut('invalidItem'), 'unknown-base': ut('unknownBase'), 'wrong-slot': ut('wrongSlot'), 'weapon-type': ut('weaponType'), 'item-level': ut('itemLevel'), 'unidentified-item': ut('unidentified') }[error.message] ?? error.message) : ut('invalidItem')); }
    finally { if (controller.current === abort) { setBusy(false); controller.current = null; } }
  };
  const apply = () => {
    if (!result) return;
    if (slot.startsWith('Jewel@')) session.setJewels([...session.jewels.filter(jewel => jewel.socket_node !== Number(slot.slice(6))), { socket_node: Number(slot.slice(6)), text: result.text }]);
    else if (/^(Flask|Charm)/.test(slot)) session.setFlasks([...session.flasks.filter(item => item.slot !== slot), { slot, text: result.text }]);
    else session.setItems([...session.items.filter(item => item.slot !== slot), { slot, text: result.text }]);
    setResult(null);
  };
  const format = (value: number) => value.toLocaleString(lang, { maximumFractionDigits: 2 });
  return <details className="upgrade-item-check ui-card">
    <summary>{ut('check')}</summary>
    <p>{ut('checkHint')}</p>
    <textarea aria-label={ut('itemText')} rows={6} value={text} onChange={event => setText(event.target.value)} placeholder="Rarity: RARE…" />
    <button className="trade-primary" disabled={busy || session.busy || !text.trim()} onClick={() => void run()}>{busy ? '…' : ut('compare')}</button>
    {error && <p role="alert" className="trade-error">{error}</p>}
    {result && <div className="upgrade-replacement-result" aria-live="polite">
      <strong className={compareObjectiveStats(result.stats, result.baseline, objective) < 0 ? 'delta-pos' : 'delta-neg'}>
        {ut(compareObjectiveStats(result.stats, result.baseline, objective) < 0 ? 'better' : 'worse')}</strong>
      {(result.stats.TotalDPS ?? 0) < (result.baseline.TotalDPS ?? 0) && <p className="delta-neg">{ut('dpsDrop')}</p>}
      {!feasibleOf(result.stats, objective) && <p className="delta-neg">{ut('constraintFail')}</p>}
      <table><thead><tr><th></th><th>{ut('before')}</th><th>{ut('after')}</th><th>{ut('change')}</th></tr></thead><tbody>
        {['TotalDPS', 'TotalEHP', 'Life', 'FireResist', 'ColdResist', 'LightningResist'].map(stat => {
          const before = result.baseline[stat] ?? 0, after = result.stats[stat] ?? 0, delta = after - before;
          return <tr key={stat}><th>{statNameLabel(lang, stat)}</th><td>{format(before)}</td><td>{format(after)}</td><td className={delta < 0 ? 'delta-neg' : delta > 0 ? 'delta-pos' : ''}>{delta > 0 ? '+' : ''}{format(delta)}</td></tr>;
        })}</tbody></table>
      {result.unsupported.length > 0 && <details className="trade-notice"><summary>{ut('incomplete')}</summary><pre>{result.unsupported.join('\n')}</pre></details>}
      <button disabled={session.busy} onClick={apply}>{ut('apply')}</button>
    </div>}
  </details>;
}
