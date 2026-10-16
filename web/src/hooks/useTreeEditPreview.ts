import { useEffect, useState } from 'react';
import { getBackend } from '../api/backend';
import type { CalculateBuildRequest, TreeAllocationMode, TreeEditAction, TreeEditResponse } from '../api/types';

/** A preview is valid only for the exact request, allocation mode and action. */
export function useTreeEditPreview(request: CalculateBuildRequest | null, node: number | null,
  mode: TreeAllocationMode, action: TreeEditAction) {
  const key = JSON.stringify([request, node, mode, action]);
  const [resolved, setResolved] = useState<{ key: string; response: TreeEditResponse } | null>(null);
  useEffect(() => {
    if (!request || node === null) return;
    let cancelled = false;
    const timer = setTimeout(() => {
      getBackend().then(backend => backend.previewTreeEdit(request, node, mode, action))
        .then(response => { if (!cancelled) setResolved({ key, response }); })
        .catch(() => { if (!cancelled) setResolved({ key, response: { valid: false, reason: 'backend_unavailable', result: null } }); });
    }, 35);
    return () => { cancelled = true; clearTimeout(timer); };
  }, [key]); // key includes every request input; stale results never become actionable.
  return resolved?.key === key ? resolved.response : null;
}
