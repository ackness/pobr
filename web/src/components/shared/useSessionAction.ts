import { useCallback, useLayoutEffect, useMemo, useRef } from 'react';
import type { BuildSession } from '../../hooks/useBuildSession';

/** Capture before awaiting a dialog/file: edits, stage switches and unmounts cancel it. */
export function useSessionAction(session: BuildSession) {
  const build = session.workspace?.builds.find(entry => entry.id === session.workspace?.activeBuild);
  const token = useMemo(() => ({}), [build?.id, build?.activeStage, session.stateVersion]);
  const current = useRef<object | null>(token);
  useLayoutEffect(() => { current.current = token; return () => { current.current = null; }; }, [token]);
  return useCallback(() => {
    const captured = current.current;
    return () => captured !== null && current.current === captured;
  }, []);
}
