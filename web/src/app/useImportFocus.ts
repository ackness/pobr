import { useCallback, useMemo, useRef } from 'react';
import type { Route } from './routes';

/** Owned by the shell: changing the active stage must not request the import keyboard again. */
export function useImportFocus(route: Route, request: number): () => boolean {
  const entry = useMemo(() => ({}), [route, request]);
  const consumed = useRef<object | null>(null);
  return useCallback(() => {
    if (route !== 'build' || consumed.current === entry) return false;
    consumed.current = entry;
    return true;
  }, [route, entry]);
}
