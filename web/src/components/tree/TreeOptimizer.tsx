import { formatApiError } from '../../api/error';
import { useMemo, useRef, useState } from 'react';
import type { VariantInput, VariantStats } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import {
  VARIANT_TOTAL_CAP,
  evaluateVariants,
  rankVariants,
  subsetCount,
  subsets,
} from '../../lib/optimize';
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
  /** 热力图结果（node skill → 单点增量）；null = 还没跑。 */
  heatData: Map<number, number> | null;
  nodeLabel: (skill: number) => string;
}

/** 候选榜长度：热力图 top-N 进勾选列表（更多手动组合意义有限）。 */
const CANDIDATE_POOL = 12;

/**
 * 天赋节点组合寻优：从热力图威力榜勾选候选，≤ 点数预算的子集经通用变体
 * 框架逐个完整重算（捕获节点间的相互作用，单点增量简单相加做不到）。
 * 不验证连通性——假设性试算，寻路由用户负责。
 */
export function TreeOptimizer({ session, lang, heatData, nodeLabel }: Props) {
  const tt = bindT(lang);
  const [expanded, setExpanded] = useState(false);
  const [ticked, setTicked] = useState<Set<number>>(new Set());
  const [points, setPoints] = useState(3);
  const [objState, setObjState] = useState(DEFAULT_OBJECTIVE_STATE);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [evaluated, setEvaluated] = useState<{
    baseline: Record<string, number>;
    results: VariantStats[];
    variants: VariantInput[];
  } | null>(null);
  const abortRef = useRef<AbortController | null>(null);

  const allocated = useMemo(() => new Set(session.allocatedNodes), [session.allocatedNodes]);
  /** 威力榜 top-N（已加点的剔除；正增量优先）。 */
  const pool = useMemo(() => {
    if (!heatData) return [];
    return [...heatData.entries()]
      .filter(([skill]) => !allocated.has(skill))
      .sort((a, b) => b[1] - a[1])
      .slice(0, CANDIDATE_POOL);
  }, [heatData, allocated]);

  const selected = pool.filter(([skill]) => ticked.has(skill)).map(([skill]) => skill);
  const combos = subsetCount(selected.length, points);
  const tooMany = combos > VARIANT_TOTAL_CAP;
  const objective = objectiveOf(objState);
  const ranked = useMemo(
    () => (evaluated ? rankVariants(evaluated.results, objective).slice(0, 10) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [evaluated, objState],
  );

  const run = async () => {
    const request = session.currentRequest();
    if (!request || selected.length === 0) return;
    const variants: VariantInput[] = subsets(selected, points).map((subset) => ({
      label: subset.map(nodeLabel).join(' + '),
      allocate_nodes: subset,
    }));
    const controller = new AbortController();
    abortRef.current = controller;
    setProgress({ done: 0, total: variants.length });
    setError(null);
    try {
      const { baseline, results } = await evaluateVariants({
        request,
        variants,
        signal: controller.signal,
        onProgress: (done, total) => setProgress({ done, total }),
      });
      setEvaluated({ baseline, results, variants });
    } catch (err: unknown) {
      setEvaluated(null);
      setError(formatApiError(err));
    } finally {
      setProgress(null);
      abortRef.current = null;
    }
  };

  const apply = (variant: VariantStats) => {
    const nodes = evaluated?.variants[variant.index]?.allocate_nodes ?? [];
    // 同一事件里连续 toggle：React 批处理成一次状态更新 → 一次重算。
    nodes.forEach((skill) => {
      if (!allocated.has(skill)) session.toggleNode(skill);
    });
    setEvaluated(null);
    setTicked(new Set());
  };

  return (
    <div className="gem-optimizer tree-optimizer">
      <button
        className="gem-optimizer-toggle"
        aria-expanded={expanded}
        onClick={() => setExpanded(!expanded)}
      >
        <span className="row-caret" aria-hidden>
          ▸
        </span>
        {tt('opt.openTree')}
      </button>
      {expanded && (
        <div className="gem-optimizer-body">
          <p className="skills-hint">{tt('opt.treeHint')}</p>
          {pool.length === 0 ? (
            <p className="skills-hint">{tt('opt.treeNeedsHeat')}</p>
          ) : (
            <>
              <div className="opt-candidates">
                {pool.map(([skill, delta]) => (
                  <label key={skill} className="opt-chip opt-node-choice">
                    <input
                      type="checkbox"
                      checked={ticked.has(skill)}
                      onChange={(e) => {
                        const next = new Set(ticked);
                        if (e.target.checked) next.add(skill);
                        else next.delete(skill);
                        setTicked(next);
                      }}
                    />
                    {nodeLabel(skill)}
                    <span className="opt-node-delta">
                      {delta >= 0 ? '+' : ''}
                      {delta.toLocaleString('en-US', { maximumFractionDigits: 1 })}
                    </span>
                  </label>
                ))}
              </div>
              <div className="opt-row">
                <label>
                  {tt('opt.points')}
                  <input
                    type="number"
                    min={1}
                    max={6}
                    value={points}
                    onChange={(e) => {
                      const v = Number(e.target.value);
                      if (Number.isInteger(v) && v >= 1 && v <= 6) setPoints(v);
                    }}
                  />
                </label>
                <ObjectiveEditor value={objState} onChange={setObjState} lang={lang} />
                <button
                  className="opt-run"
                  disabled={session.busy || progress !== null || selected.length === 0 || tooMany}
                  onClick={run}
                >
                  {progress
                    ? tt('opt.running')
                    : `${tt('opt.run')}（${combos} ${tt('opt.combos')}）`}
                </button>
              </div>
              {tooMany && <p className="opt-error">{tt('opt.tooMany')}</p>}
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
            </>
          )}
        </div>
      )}
    </div>
  );
}
