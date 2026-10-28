import { useEffect } from 'react';
import { jobEnded, jobStarted } from './activeJobs';

/**
 * Lets a component that runs its own long work (an optimizer with its own AbortController and
 * progress) light the top bar's progress line while `active`, without changing how it runs or cancels.
 */
export function useJobBeacon(active: boolean) {
  useEffect(() => {
    if (!active) return;
    jobStarted();
    return () => { jobEnded(); };
  }, [active]);
}
