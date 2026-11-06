import { useEffect, useState } from 'react';
import { getBackend } from '../api/backend';
import type { GemDescriptionIndex } from '../lib/gemDescriptions';

let loggedLoadError = false;

/** Loading and absent/error packs keep the original picker and recommendation UI. */
export function useGemDescriptions(): GemDescriptionIndex | null | undefined {
  const [index, setIndex] = useState<GemDescriptionIndex | null>();
  useEffect(() => {
    let cancelled = false;
    getBackend().then(backend => backend.gemDescriptions()).then(value => {
      if (!cancelled) setIndex(value);
    }).catch((error: unknown) => {
      if (!loggedLoadError) {
        loggedLoadError = true;
        console.warn('Gem descriptions unavailable.', error);
      }
      if (!cancelled) setIndex(null);
    });
    return () => { cancelled = true; };
  }, []);
  return index;
}
