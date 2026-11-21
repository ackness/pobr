import { DisclosureChevron } from '../shared/DisclosureChevron';
import { useJobBeacon } from '../../app/jobs/useJobBeacon';
import { AppButton } from '../shared/controls';
import { formatApiError } from '../../api/error';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useUpgradeGoal } from '../../hooks/useUpgradeGoal';
import type { VariantInput, VariantStats } from '../../api/types';
import type { BuildSession, LibraryItem } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import { useHeadlineDamage } from '../../hooks/useMainMinion';
import { resolveDamageObjective } from '../../lib/mainMinion';
import { evaluateVariants, rankVariants } from '../../lib/optimize';
import {
  ObjectiveEditor,
  OptimizerProgress,
  OptimizerResults,
  objectiveOf,
} from '../shared/OptimizerControls';

interface Props {
  session: BuildSession;
  lang: Lang;
  /** Selected equipment slot ID (utility slots Flask/Charm do not enter this component). */
  slot: string;
  /** Slot-matching candidates from the item library (pre-filtered by ItemsPanel). */
  candidates: LibraryItem[];
  /** Localized display names aligned with `candidates`. */
  candidateNames: string[];
  /** Current text in the slot (`undefined` when empty). */
  currentText: string | undefined;
  /** Apply handler: `null` removes the item, `string` swaps to that text. */
  onApply: (text: string | null) => void;
}

/**
 * Per-slot candidate optimizer: trials each library item matching the slot (plus an "Unequip" variant)
 * through full variant recalculation, ranks by objective/constraints, and applies with one click.
 */
export function ItemOptimizer({
  session,
  lang,
  slot,
  candidates,
  candidateNames,
  currentText,
  onApply,
}: Props) {
  const tt = bindT(lang);
  const damage = useHeadlineDamage(session.calc, lang);
  const [expanded, setExpanded] = useState(false);
  const { goal: objState, setGoal: setObjState } = useUpgradeGoal();
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  useJobBeacon(progress !== null);
  const [error, setError] = useState<string | null>(null);
  const [evaluated, setEvaluated] = useState<{
    baseline: Record<string, number>;
    results: VariantStats[];
    variants: VariantInput[];
  } | null>(null);
  const abortRef = useRef<AbortController | null>(null);

  useEffect(() => { abortRef.current?.abort(); setEvaluated(null); setProgress(null); }, [session.currentRequest, slot]);
  useEffect(() => () => abortRef.current?.abort(), []);

  const variantDefs = useMemo<VariantInput[]>(() => {
    const defs: VariantInput[] = candidates
      .filter((c) => c.text !== currentText)
      .map((c) => ({
        label: candidateNames[candidates.indexOf(c)] || c.name,
        set_items: [{ slot, text: c.text }],
      }));
    if (currentText) {
      defs.push({ label: tt('items.unequip'), set_items: [{ slot, text: '' }] });
    }
    return defs;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [candidates, candidateNames, currentText, slot, lang]);

  const objective = useMemo(() => resolveDamageObjective(objectiveOf(objState, evaluated?.baseline), session.calc), [objState, evaluated?.baseline, session.calc]);
  const ranked = useMemo(
    () => (evaluated ? rankVariants(evaluated.results, objective) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [evaluated, objective],
  );

  const run = async () => {
    const request = session.currentRequest();
    if (!request || variantDefs.length === 0) return;
    const controller = new AbortController();
    abortRef.current = controller;
    setProgress({ done: 0, total: variantDefs.length });
    setError(null);
    try {
      const { baseline, results } = await evaluateVariants({
        request,
        variants: variantDefs,
        signal: controller.signal,
        onProgress: (done, total) => { if (!controller.signal.aborted) setProgress({ done, total }); },
      });
      if (!controller.signal.aborted) setEvaluated({ baseline, results, variants: variantDefs });
    } catch (err: unknown) {
      if (!controller.signal.aborted) { setEvaluated(null); setError(formatApiError(err)); }
    } finally {
      if (abortRef.current === controller) { setProgress(null); abortRef.current = null; }
    }
  };

  const apply = (variant: VariantStats) => {
    const item = evaluated?.variants[variant.index]?.set_items?.[0];
    if (!item) return;
    onApply(item.text === '' ? null : item.text);
    setEvaluated(null);
  };

  return (
    <div className="gem-optimizer">
      <AppButton
        className="gem-optimizer-toggle disclosure" data-disclosure-level="section"
        aria-expanded={expanded}
        onClick={() => setExpanded(!expanded)}
      >
        <DisclosureChevron className="row-caret" />
        {tt('opt.openItem')}
      </AppButton>
      {expanded && (
        <div className="gem-optimizer-body">
          <p className="skills-hint">{tt('opt.itemHint')}</p>
          {variantDefs.length === 0 ? (
            <p className="skills-hint">{tt('opt.needLibrary')}</p>
          ) : (
            <div className="opt-row">
              <ObjectiveEditor damage={damage} value={objState} onChange={setObjState} lang={lang} />
              <AppButton
                className="opt-run"
                disabled={session.busy || progress !== null}
                onClick={run}
              >
                {progress
                  ? tt('opt.running')
                  : `${tt('opt.run')}（${variantDefs.length} ${tt('opt.combos')}）`}
              </AppButton>
            </div>
          )}
          {progress && (
            <OptimizerProgress
              done={progress.done}
              total={progress.total}
              onCancel={() => abortRef.current?.abort()}
              lang={lang}
            />
          )}
          {error && <p className="opt-error">{error}</p>}
          {evaluated && (
            <OptimizerResults
              baseline={evaluated.baseline}
              ranked={ranked}
              objective={objective}
              onApply={apply}
              lang={lang}
              disabled={session.busy}
            />
          )}
        </div>
      )}
    </div>
  );
}
