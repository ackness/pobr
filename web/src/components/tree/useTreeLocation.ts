import { useEffect, type RefObject } from 'react';
import type { PassiveNode } from '../../api/types';

/** The request owner retains the receipt while the canvas may unmount or change weapon sets. */
export function useTreeLocation(request: { nonce: number; node?: number } | undefined,
  consumed: RefObject<number | undefined>, node: PassiveNode | undefined, ready: boolean,
  focus: (node: PassiveNode) => void) {
  useEffect(() => {
    if (!request || consumed.current === request.nonce || !ready
      || node?.x === undefined || node.y === undefined) return;
    consumed.current = request.nonce;
    focus(node);
  }, [request?.nonce, node, ready]);
}
