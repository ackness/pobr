import { useEffect, useState } from 'react';
import { getBackend } from '../api/backend';
import type { GemDescriptionIndex } from '../lib/gemDescriptions';

let loggedLoadError = false;
let loading: Promise<GemDescriptionIndex | null> | undefined;

// Share the large Worker response on the page as well as the backend's file/index cache.
function loadDescriptions(): Promise<GemDescriptionIndex | null> {
  return loading ??= getBackend().then(backend => backend.gemDescriptions()).catch((error: unknown) => {
    loading = undefined;
    throw error;
  });
}

/** Loading and absent/error packs keep the original picker and recommendation UI. */
export function useGemDescriptions(): GemDescriptionIndex | null | undefined {
  const [index, setIndex] = useState<GemDescriptionIndex | null>();
  useEffect(() => {
    let cancelled = false;
    loadDescriptions().then(value => {
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
