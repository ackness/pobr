import { useEffect, useRef, useState } from 'react';
import type { AttributionResponse } from '../../api/types';
import { formatApiError } from '../../api/error';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import { formatStatValue } from '../../lib/statDisplay';
import { AppButton } from '../shared/controls';

export const contributionFields = ['CombinedDPS', 'Life', 'EnergyShield', 'TotalEHP'];

export function ItemContribution({ session, slot, lang }: { session: BuildSession; slot: string; lang: Lang }) {
  const tt = bindT(lang);
  const [completed, setCompleted] = useState<{ version: number; report: AttributionResponse } | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const sequence = useRef(0);
  useEffect(() => {
    sequence.current++; setRunning(false); setError(null);
    return () => { sequence.current++; };
  }, [session.stateVersion]);
  const report = session.sourceContributions ?? (completed?.version === session.stateVersion ? completed.report : null);
  const kind = slot.startsWith('Flask') || slot.startsWith('Charm') ? 'flask' : /^(arm|leg)/.test(slot) ? 'limb' : 'item';
  const entry = report?.entries.find(entry => entry.kind === kind && entry.id === slot);
  const available = entry && ['CombinedDPS', 'TotalEHP'].every(field => Number.isFinite(entry.deltas[field]));
  const run = async () => {
    const current = ++sequence.current, version = session.stateVersion;
    setRunning(true); setError(null);
    try {
      const report = await session.runAttribution(contributionFields);
      if (current === sequence.current) setCompleted({ version, report });
    } catch (cause) {
      if (current === sequence.current) setError(formatApiError(cause));
    } finally { if (current === sequence.current) setRunning(false); }
  };
  return <section className="item-contribution" aria-label={tt('items.contribution')}>
    <h4>{tt('items.contribution')}</h4>
    {available ? <p>{tt('items.removalLoss').replace('{dps}', formatStatValue(entry.deltas.CombinedDPS, 'float2', lang))
      .replace('{ehp}', formatStatValue(entry.deltas.TotalEHP, 'float2', lang))}</p> : <>
      <p>{tt('items.contributionMissing')}</p>
      <AppButton onClick={() => void run()} disabled={running || session.busy || !session.calc}>
        {tt(running ? 'calcs.running' : 'calcs.runAttribution')}
      </AppButton>
    </>}
    <p className="items-hint">{tt('items.contributionHint')}</p>
    {error && <p role="alert">{error}</p>}
  </section>;
}
