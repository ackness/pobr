import { createContext, useContext } from 'react';
import type { CalculateBuildRequest } from '../../api/types';
import type { StatChange } from '../../app/motion/statChanges';

export interface PreviewState { label: string; status: 'loading' | 'ready'; changes: StatChange[] }
export interface PreviewCandidate {
  /** Identifies the candidate for caching within one build state. */
  key: string;
  label: string;
  /** Builds the request lazily, so nothing is computed until the pointer has rested on the candidate. */
  request: () => CalculateBuildRequest | null;
}
export interface PreviewApi {
  state: PreviewState | null;
  /** Calculates a candidate on its own and shows what it would change. */
  show: (candidate: PreviewCandidate) => void;
  /** Shows changes that are already known, such as a ranked result's stats, without calculating anything. */
  showChanges: (label: string, changes: StatChange[]) => void;
  clear: () => void;
}

/** Provided by the app shell; where it is absent (a panel rendered on its own) `usePreview()` is null and hovering does nothing. */
export const PreviewContext = createContext<PreviewApi | null>(null);
export function usePreview() { return useContext(PreviewContext); }
