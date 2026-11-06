import { useEffect, useState } from 'react';
import { getBackend } from '../../../api/backend';
import type { SkillMetricsEntry } from '../../../api/types';
import type { BuildSession } from '../../../hooks/useBuildSession';

export interface SkillMetricsSnapshot {
  /** Group indices are only meaningful for the revision they were measured against. */
  revision: number;
  skills: SkillMetricsEntry[];
}

let latestMetrics: {
  request: BuildSession['currentRequest'];
  version: number;
  revision: number;
  result: Promise<SkillMetricsSnapshot>;
} | undefined;

function sharedMetrics(session: BuildSession): Promise<SkillMetricsSnapshot> | undefined {
  const { currentRequest, stateVersion, socketGroupRevision } = session;
  if (latestMetrics?.request === currentRequest && latestMetrics.version === stateVersion
    && latestMetrics.revision === socketGroupRevision) return latestMetrics.result;
  const request = currentRequest();
  if (!request) return;
  const entry = {
    request: currentRequest, version: stateVersion, revision: socketGroupRevision,
    result: getBackend().then(backend => backend.skillMetrics(request))
      .then(result => ({ revision: socketGroupRevision, skills: result.skills })),
  };
  latestMetrics = entry;
  // A failed batch must not prevent a later consumer from retrying the same state.
  void entry.result.catch(() => { if (latestMetrics === entry) latestMetrics = undefined; });
  return entry.result;
}

/**
 * Every active effect calculated on its own, kept up to date as the build is edited.
 * The previous result stays in place while the next one is computed so a list never blinks
 * on each keystroke. A removed or replaced group invalidates the indices, so such a result
 * is withheld by the caller (compare `revision` with `session.socketGroupRevision`).
 * Nothing is requested while `enabled` is false, for example on a page that is not showing.
 */
export function useSkillMetrics(session: BuildSession, enabled: boolean, delay = 250): SkillMetricsSnapshot | null {
  const [snapshot, setSnapshot] = useState<SkillMetricsSnapshot | null>(null);
  const hasGroups = session.socketGroups.length > 0;
  useEffect(() => { if (!hasGroups) setSnapshot(null); }, [hasGroups]);
  useEffect(() => {
    if (!enabled || session.busy || session.calcError || !hasGroups) return;
    let stale = false;
    const timer = setTimeout(() => {
      sharedMetrics(session)
        ?.then(result => { if (!stale) setSnapshot(result); })
        .catch(() => { if (!stale) setSnapshot(null); });
    }, delay);
    return () => { stale = true; clearTimeout(timer); };
    // The version number is the change signal; the request is read at the moment it is sent.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled, session.stateVersion, session.busy, session.calcError, hasGroups, delay]);
  return snapshot;
}
