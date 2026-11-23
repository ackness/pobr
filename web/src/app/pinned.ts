import { useCallback, useState } from 'react';
import { readPreference, uiStorageKey, writePreference } from '../lib/sessionStorage';

const KEY = uiStorageKey('pobr-rail-pinned');
export const MAX_PINNED = 12;

/** Adds a stat to the pinned list, or removes it if it is already there. The newest pin goes last and the list is capped. */
export function togglePinned(list: readonly string[], id: string, max = MAX_PINNED): string[] {
  if (list.includes(id)) return list.filter(item => item !== id);
  return [...list, id].slice(-max);
}

function read(): string[] {
  try { const value = JSON.parse(readPreference(KEY) ?? '[]'); return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string').slice(0, MAX_PINNED) : []; }
  catch { return []; }
}

/** The stats a player chose to keep at the top of the results rail, remembered on this device. */
export function usePinnedStats() {
  const [pinned, setPinned] = useState<string[]>(read);
  const toggle = useCallback((id: string) => setPinned(previous => {
    const next = togglePinned(previous, id);
    writePreference(KEY, JSON.stringify(next));
    return next;
  }), []);
  return { pinned, toggle };
}
