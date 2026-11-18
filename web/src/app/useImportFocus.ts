import { useCallback, useRef } from 'react';
import type { Route } from './routes';

/** Owned by the shell: changing the active stage must not request the import keyboard again. */
export function useImportFocus(route: Route, request: number): () => boolean {
  const consumed = useRef(0);
  return useCallback(() => {
    if (route !== 'build' || request <= consumed.current) return false;
    consumed.current = request;
    return true;
  }, [route, request]);
}
