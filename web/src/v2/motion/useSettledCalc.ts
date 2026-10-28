import { useState } from 'react';
import type { CalculateBuildResponse } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';

export interface SettledCalc {
  /** The latest calculation that matches a build state, kept while the next one is pending. */
  calc: CalculateBuildResponse | null;
  /** The settled calculation before `calc` for the same scope, for showing what an edit moved. */
  previous: CalculateBuildResponse | null;
  /** A newer calculation is on its way, or the last one failed. Numbers stay, dimmed. */
  stale: boolean;
}

/**
 * `session.calc` is null for as long as a recalculation is pending, which would blank every number
 * on each edit. This keeps the last good result instead. A failed calculation is not a valid
 * baseline, and results never carry across `scope` (a different build or stage).
 */
export function useSettledCalc(session: BuildSession, scope: string | undefined): SettledCalc {
  const [held, setHeld] = useState<{ scope?: string; calc: CalculateBuildResponse; previous: CalculateBuildResponse | null } | null>(null);
  const fresh = session.calcError ? null : session.calc;
  if (fresh && (held?.calc !== fresh || held.scope !== scope)) {
    setHeld({ scope, calc: fresh, previous: held && held.scope === scope ? held.calc : null });
  }
  const usable = held && held.scope === scope && !session.calcError ? held : null;
  return { calc: usable?.calc ?? null, previous: usable?.previous ?? null, stale: session.busy || !session.calc || !!session.calcError };
}
