import { useEffect, useState } from 'react';
import type { AttributeChoice, CalculateBuildRequest, CalculateBuildResponse, TreeEditResponse } from '../../api/types';
import { getBackend } from '../../api/backend';
import { diffStats, type DiffEntry } from '../../lib/compare';
import { bindT, type Lang } from '../../lib/i18n';
import { treeAllocationRequest } from '../../lib/weaponSets';
import { TreeHoverBenefits } from './TreeHoverBenefits';

/** The same validated full-route changes are available from the keyboard's node inspector. */
export function TreeNodeChanges({ request, preview, baseline, busy, allocated, lang, attributeChoices }: {
  request: CalculateBuildRequest | null; preview: TreeEditResponse | null;
  attributeChoices?: Record<string, AttributeChoice>;
  baseline: CalculateBuildResponse | null; busy: boolean; allocated: boolean; lang: Lang;
}) {
  const tt = bindT(lang);
  const calculationRequest = request && preview?.result ? treeAllocationRequest({ ...request,
    attribute_choices: { ...request.attribute_choices, ...attributeChoices },
  }, preview.result.allocation) : null;
  const key = JSON.stringify([calculationRequest, preview, baseline?.stats]);
  const [resolved, setResolved] = useState<{ key: string; diffs: DiffEntry[] | null } | null>(null);
  const ready = !busy && Boolean(request && baseline && preview?.valid && preview.result);
  useEffect(() => {
    if (!ready) return;
    let cancelled = false;
    const timer = setTimeout(() => {
      getBackend().then(backend => backend.calculateBuild(calculationRequest!))
        .then(next => { if (!cancelled) setResolved({ key, diffs: diffStats(baseline!.stats, next.stats) }); })
        .catch(() => { if (!cancelled) setResolved({ key, diffs: null }); });
    }, 300);
    return () => { cancelled = true; clearTimeout(timer); };
  }, [key, ready]);
  const result = ready && resolved?.key === key ? resolved : null;
  return <details className="tree-node-changes">
    <summary>{tt(allocated ? 'diff.ifDealloc' : 'diff.ifAlloc')}</summary>
    {result?.diffs ? <TreeHoverBenefits diffs={result.diffs} lang={lang} refundCount={preview?.result?.removed.length} />
      : <p role="status">{busy || !preview || ready && !result ? tt('tree.deltaPending') : tt('tree.deltaUnavailable')}</p>}
  </details>;
}
