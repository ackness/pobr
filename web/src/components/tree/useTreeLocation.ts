import { useEffect, type RefObject } from 'react';
import type { PassiveNode } from '../../api/types';
import { useConsumedRequest } from '../../hooks/useConsumedRequest';

/** The request owner retains the receipt while the canvas may unmount or change weapon sets. */
export function useTreeLocation(request: { nonce: number; node?: number } | undefined,
  consumed: RefObject<number | undefined>, node: PassiveNode | undefined, ready: boolean,
  focus: (node: PassiveNode) => void) {
  const locate = useConsumedRequest(ready && node?.x !== undefined && node.y !== undefined,
    request?.nonce, () => { if (node) focus(node); }, consumed);
  useEffect(locate, [locate]);
}
