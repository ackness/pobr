import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { getBackend } from '../../api/backend';
import { PreviewContext, type PreviewCandidate, type PreviewState } from '../../components/shared/PreviewContext';
import type { BuildSession } from '../../hooks/useBuildSession';
import { computeStatChanges, type StatChange } from '../motion/statChanges';
import type { SettledCalc } from '../motion/useSettledCalc';
import { summarizeChanges } from './changeSummary';

const REST_MS = 120;
const HIDE_MS = 100;

/**
 * Hover = what-if. A candidate under the pointer is calculated on its own and its effect shown without
 * touching the build: debounced, cancelled when the pointer moves on, remembered per build state. It never
 * goes through the session, so `session.busy` stays false and nothing else is locked while you look.
 */
export function PreviewProvider({ session, view, route, children }: { session: BuildSession; view: SettledCalc; route?: string; children: ReactNode }) {
  const [state, setState] = useState<PreviewState | null>(null);
  const sequence = useRef(0);
  const target = useRef<Element | undefined>(undefined);
  const observer = useRef<MutationObserver | null>(null);
  const timers = useRef<{ rest?: number; hide?: number }>({});
  const cache = useRef<{ version: number; rows: Map<string, StatChange[]> }>({ version: -1, rows: new Map() });
  const latest = useRef({ session, view });
  latest.current = { session, view };

  const disconnectTarget = useCallback(() => {
    observer.current?.disconnect();
    observer.current = null;
    target.current = undefined;
  }, []);
  const reset = useCallback(() => {
    window.clearTimeout(timers.current.rest); window.clearTimeout(timers.current.hide);
    sequence.current++; disconnectTarget(); setState(null);
  }, [disconnectTarget]);
  const watchTarget = useCallback((element?: Element) => {
    disconnectTarget();
    if (!element) return;
    target.current = element;
    observer.current = new MutationObserver(() => {
      const current = target.current;
      if (current && (!current.isConnected || current.closest('[hidden], [inert]'))) reset();
    });
    observer.current.observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ['hidden', 'inert'] });
  }, [disconnectTarget, reset]);

  const show = useCallback((candidate: PreviewCandidate, element?: Element) => {
    watchTarget(element);
    window.clearTimeout(timers.current.hide); window.clearTimeout(timers.current.rest);
    const id = ++sequence.current;
    timers.current.rest = window.setTimeout(async () => {
      if (sequence.current !== id) return;
      const { session: current, view: baseline } = latest.current;
      if (!baseline.calc || current.busy) return;
      const request = candidate.request();
      if (!request) return;
      if (cache.current.version !== current.stateVersion) cache.current = { version: current.stateVersion, rows: new Map() };
      const known = cache.current.rows.get(candidate.key);
      if (known) { setState({ label: candidate.label, status: 'ready', changes: known }); return; }
      setState({ label: candidate.label, status: 'loading', changes: [] });
      try {
        const result = await (await getBackend()).calculateBuild({ ...request, breakdowns: [] });
        if (sequence.current !== id) return;
        const changes = summarizeChanges(computeStatChanges(baseline.calc.stats, result.stats));
        cache.current.rows.set(candidate.key, changes);
        setState({ label: candidate.label, status: 'ready', changes });
      } catch { if (sequence.current === id) setState(null); }
    }, REST_MS);
  }, [watchTarget]);
  const showChanges = useCallback((label: string, changes: StatChange[], element?: Element) => {
    watchTarget(element);
    window.clearTimeout(timers.current.hide); window.clearTimeout(timers.current.rest);
    sequence.current++;
    setState({ label, status: 'ready', changes });
  }, [watchTarget]);
  const clear = useCallback(() => {
    window.clearTimeout(timers.current.hide); window.clearTimeout(timers.current.rest); sequence.current++; disconnectTarget();
    timers.current.hide = window.setTimeout(() => setState(null), HIDE_MS);
  }, [disconnectTarget]);
  useEffect(() => reset, [reset]);
  // Navigation, including retained panels, invalidates all pending preview work.
  useEffect(reset, [session.stateVersion, route, reset]);
  const value = useMemo(() => ({ state, show, showChanges, clear }), [state, show, showChanges, clear]);
  return <PreviewContext.Provider value={value}>{children}</PreviewContext.Provider>;
}
