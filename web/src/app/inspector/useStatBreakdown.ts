import { useEffect, useState } from 'react';
import { formatApiError } from '../../api/error';
import { getBackend } from '../../api/backend';
import type { Breakdown } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { StatDetailCache } from '../../lib/statDetails';
import { parseStatId } from './statId';

/** One cache for every inspector view; it drops everything whenever the calculation it belongs to changes. */
const cache = new StatDetailCache();

export interface StatBreakdownState { detail: Breakdown | null; error?: string; loading: boolean; retry: () => void }

/**
 * The calculation steps behind one stat, fetched lazily (never with the baseline calculation) and
 * kept on screen, dimmed by the caller, while a newer calculation is pending.
 */
export function useStatBreakdown(session: BuildSession, id: string): StatBreakdownState {
  const [loaded, setLoaded] = useState<{ id: string; calc: object; detail: Breakdown | null; error?: string } | null>(null);
  const [attempt, setAttempt] = useState(0);
  const calc = session.calc;
  useEffect(() => {
    if (!calc || session.busy) return;
    const request = session.currentRequest();
    if (!request) return;
    const { stat, minion } = parseStatId(id);
    let cancelled = false;
    cache.load(calc, id, async () => {
      const result = await (await getBackend()).calculateBuild({ ...request, breakdowns: [stat], breakdown_actor: minion === undefined ? { kind: 'player' } : { kind: 'minion', index: minion } });
      return (minion === undefined ? result.breakdowns[stat] : result.minions?.find(row => row.index === minion)?.breakdowns[stat]) ?? null;
    }).then(detail => { if (!cancelled) setLoaded({ id, calc, detail }); })
      .catch(error => { if (!cancelled) setLoaded({ id, calc, detail: null, error: formatApiError(error) }); });
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `currentRequest` is read at the moment the calculation settles.
  }, [id, calc, session.busy, attempt]);
  const current = loaded?.id === id ? loaded : null;
  return { detail: current?.detail ?? null, error: current?.error, loading: !current || current.calc !== calc, retry: () => { setLoaded(null); setAttempt(value => value + 1); } };
}
