import { useEffect, useState } from 'react';
import { getBackend } from '../../../api/backend';
import type { BuildSession } from '../../../hooks/useBuildSession';
import { spiritReservations, type SpiritReservationReport } from './spiritReservations';

export function useSpiritReservations(session: BuildSession, enabled: boolean, refresh = 0) {
  const [snapshot, setSnapshot] = useState<{ version: number; rows: SpiritReservationReport | null }>();
  useEffect(() => {
    if (!enabled || session.busy || session.calcError) return;
    const request = session.currentRequest();
    if (!request) return;
    const controller = new AbortController();
    const version = session.stateVersion;
    const timer = setTimeout(() => {
      getBackend().then(backend => spiritReservations(request, candidate => backend.calculateBuild(candidate), controller.signal))
        .then(rows => { if (!controller.signal.aborted) setSnapshot({ version, rows }); })
        .catch(() => { if (!controller.signal.aborted) setSnapshot({ version, rows: null }); });
    }, 250);
    return () => { clearTimeout(timer); controller.abort(); };
    // A snapshot is valid only for this exact build revision.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled, refresh, session.stateVersion, session.busy, session.calcError]);
  return snapshot?.version === session.stateVersion ? snapshot.rows : undefined;
}
