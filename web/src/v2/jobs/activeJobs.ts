import { useSyncExternalStore } from 'react';

let count = 0;
const listeners = new Set<() => void>();
const notify = () => listeners.forEach(listener => listener());
export function jobStarted() { count++; notify(); }
export function jobEnded() { count = Math.max(0, count - 1); notify(); }
export function activeJobCount() { return count; }
const subscribe = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };
/** How many long-running jobs are in flight; the top bar's progress line follows it. */
export function useActiveJobCount() { return useSyncExternalStore(subscribe, activeJobCount, () => 0); }
