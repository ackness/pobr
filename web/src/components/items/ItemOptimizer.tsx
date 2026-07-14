import { useMemo, useRef, useState } from 'react';
import type { VariantInput, VariantStats } from '../../api/types';
import type { BuildSession, LibraryItem } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import { evaluateVariants, rankVariants } from '../../lib/optimize';
import {
  DEFAULT_OBJECTIVE_STATE,
  ObjectiveEditor,
  OptimizerProgress,
  OptimizerResults,
  objectiveOf,
} from '../shared/OptimizerControls';

interface Props {
  session: BuildSession;
  lang: Lang;
  /** 选中的装备槽 id（工具槽 Flask/Charm 不进此组件）。 */
  slot: string;
  /** 物品库中槽位匹配的候选（ItemsPanel 已过滤）。 */
  candidates: LibraryItem[];
  /** 与 candidates 对齐的本地化显示名。 */
  candidateNames: string[];
  /** 当前槽内文本（空槽为 undefined）。 */
  currentText: string | undefined;
  /** 应用结果：null = 摘下，string = 换装该文本。 */
  onApply: (text: string | null) => void;
}

/**
 * 槽位候选装备寻优：物品库同槽候选逐件试穿（含「摘下」变体）走通用变体
 * 框架完整重算，按目标/约束排名，一键换装。
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
  const [expanded, setExpanded] = useState(false);
  const [objState, setObjState] = useState(DEFAULT_OBJECTIVE_STATE);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [evaluated, setEvaluated] = useState<{
    baseline: Record<string, number>;
    results: VariantStats[];
    variants: VariantInput[];
  } | null>(null);
  const abortRef = useRef<AbortController | null>(null);

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

  const objective = objectiveOf(objState);
  const ranked = useMemo(
    () => (evaluated ? rankVariants(evaluated.results, objective) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [evaluated, objState],
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
        onProgress: (done, total) => setProgress({ done, total }),
      });
      setEvaluated({ baseline, results, variants: variantDefs });
    } catch (err: unknown) {
      setEvaluated(null);
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setProgress(null);
      abortRef.current = null;
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
      <button
        className="gem-optimizer-toggle"
        aria-expanded={expanded}
        onClick={() => setExpanded(!expanded)}
      >
        <span className="row-caret" aria-hidden>
          ▸
        </span>
        {tt('opt.openItem')}
      </button>
      {expanded && (
        <div className="gem-optimizer-body">
          <p className="skills-hint">{tt('opt.itemHint')}</p>
          {variantDefs.length === 0 ? (
            <p className="skills-hint">{tt('opt.needLibrary')}</p>
          ) : (
            <div className="opt-row">
              <ObjectiveEditor value={objState} onChange={setObjState} lang={lang} />
              <button
                className="opt-run"
                disabled={session.busy || progress !== null}
                onClick={run}
              >
                {progress
                  ? tt('opt.running')
                  : `${tt('opt.run')}（${variantDefs.length} ${tt('opt.combos')}）`}
              </button>
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
