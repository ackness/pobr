import { useRef, type RefObject } from 'react';

/** Return an effect callback so callers retain their layout/passive effect timing. */
export function useConsumedRequest(active: boolean, nonce: number | undefined, run: () => void,
  receipt?: RefObject<number | undefined>): () => void {
  const localReceipt = useRef<number | undefined>(undefined);
  const consumed = receipt ?? localReceipt;
  return () => {
    if (!active || nonce === undefined || consumed.current === nonce) return;
    consumed.current = nonce;
    run();
  };
}
