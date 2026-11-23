import { useEffect } from 'react';
import type { AttributionResponse } from '../../api/types';
import { formatApiError } from '../../api/error';
import { useCancellableJob } from '../../app/jobs/useCancellableJob';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import { formatStatValue } from '../../lib/statDisplay';
import { AppButton } from '../shared/controls';

export const contributionFields = ['CombinedDPS', 'Life', 'EnergyShield', 'TotalEHP'];

export function ItemContribution({ session, slot, lang }: { session: BuildSession; slot: string; lang: Lang }) {
  const tt = bindT(lang);
  const job = useCancellableJob<{ version: number; report: AttributionResponse }>();
  useEffect(() => {
    job.reset();
  }, [session.stateVersion, job.reset]);
  const completed = job.result;
  const error = job.error ? formatApiError(job.error) : null;
  const report = session.sourceContributions ?? (completed?.version === session.stateVersion ? completed.report : null);
  const kind = slot.startsWith('Flask') || slot.startsWith('Charm') ? 'flask' : /^(arm|leg)/.test(slot) ? 'limb' : 'item';
  const entry = report?.entries.find(entry => entry.kind === kind && entry.id === slot);
  const available = entry && ['CombinedDPS', 'TotalEHP'].every(field => Number.isFinite(entry.deltas[field]));
  const run = () => {
    const version = session.stateVersion;
    return job.start(async () => ({ version, report: await session.runAttribution(contributionFields) }));
  };
  return <section className="item-contribution" aria-label={tt('items.contribution')}>
    <h4>{tt('items.contribution')}</h4>
    {available ? <p>{tt('items.removalLoss').replace('{dps}', formatStatValue(entry.deltas.CombinedDPS, 'float2', lang))
      .replace('{ehp}', formatStatValue(entry.deltas.TotalEHP, 'int', lang))}</p> : <>
      <p>{tt('items.contributionMissing')}</p>
      <AppButton onClick={run} disabled={job.running || session.busy || !session.calc}>
        {tt(job.running ? 'calcs.running' : 'calcs.runAttribution')}
      </AppButton>
    </>}
    <p className="items-hint">{tt('items.contributionHint')}</p>
    {error && <p role="alert">{error}</p>}
  </section>;
}
