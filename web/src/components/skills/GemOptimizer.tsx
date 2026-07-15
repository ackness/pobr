import { formatApiError } from '../../api/error';
import { useMemo, useRef, useState } from 'react';
import type { GemCatalogEntry, GemInput, VariantInput, VariantStats } from '../../api/types';
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
import { GemPicker } from './GemPicker';

interface Props {
  session: BuildSession;
  lang: Lang;
  groupIndex: number;
  /** 目录里的全部辅助宝石（GemPicker 候选源）。 */
  supports: GemCatalogEntry[];
  gemName: (skillId: string) => string;
}

/**
 * 技能组辅助宝石寻优：候选池的非空子集（≤ 空槽数）经通用变体框架逐个完整
 * 重算；打分/排序在前端——评估一次后切换目标即时重排零重算。
 */
export function GemOptimizer({ session, lang, groupIndex, supports, gemName }: Props) {
  const tt = bindT(lang);
  const [expanded, setExpanded] = useState(false);
  const [candidates, setCandidates] = useState<GemInput[]>([]);
  const [freeSlots, setFreeSlots] = useState(2);
  const [objState, setObjState] = useState(DEFAULT_OBJECTIVE_STATE);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [evaluated, setEvaluated] = useState<{
    baseline: Record<string, number>;
    results: VariantStats[];
    variants: VariantInput[];
  } | null>(null);
  const abortRef = useRef<AbortController | null>(null);

  const group = session.socketGroups[groupIndex];
  const pickerEntries = useMemo(() => {
    const taken = new Set([
      ...candidates.map((c) => c.skill_id),
      ...(group?.gems ?? []).map((g) => g.skill_id),
    ]);
    return supports.filter((e) => !taken.has(e.skill_id));
  }, [supports, candidates, group]);

  const combos = subsetCount(candidates.length, freeSlots);
  const tooMany = combos > VARIANT_TOTAL_CAP;
  const objective = objectiveOf(objState);
  const ranked = useMemo(
    () => (evaluated ? rankVariants(evaluated.results, objective).slice(0, 10) : []),
    // objective 是每次渲染的新对象，用其来源 objState 作依赖。
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [evaluated, objState],
  );

  const run = async () => {
    const request = session.currentRequest();
    if (!request || candidates.length === 0) return;
    const variants: VariantInput[] = subsets(candidates, freeSlots).map((subset) => ({
      label: subset.map((g) => gemName(g.skill_id)).join(' + '),
      add_gems: { group_index: groupIndex, gems: subset },
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
    const gems = evaluated?.variants[variant.index]?.add_gems?.gems ?? [];
    session.setSocketGroups(
      session.socketGroups.map((g, i) =>
        i === groupIndex ? { ...g, gems: [...g.gems, ...gems] } : g,
      ),
    );
    setEvaluated(null);
    setCandidates([]);
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
        {tt('opt.open')}
      </button>
      {expanded && (
        <div className="gem-optimizer-body">
          <p className="skills-hint">{tt('opt.hint')}</p>
          <div className="opt-candidates">
            {candidates.map((c) => {
              const entry = supports.find((e) => e.skill_id === c.skill_id);
              return (
                <span key={c.skill_id} className="opt-chip">
                  {gemName(c.skill_id)}
                  {entry?.is_lineage && (
                    <span className="gem-lineage-badge">{tt('picker.lineage')}</span>
                  )}
                  <button
                    className="skill-remove"
                    title={tt('skills.removeGem')}
                    onClick={() =>
                      setCandidates(candidates.filter((x) => x.skill_id !== c.skill_id))
                    }
                  >
                    ×
                  </button>
                </span>
              );
            })}
            <GemPicker
              entries={pickerEntries}
              placeholder={tt('opt.addCandidate')}
              disabled={session.busy}
              lang={lang}
              onPick={(skillId) =>
                setCandidates([...candidates, { skill_id: skillId, level: 20, quality: 0 }])
              }
            />
          </div>
          <div className="opt-row">
            <label>
              {tt('opt.freeSlots')}
              <input
                type="number"
                min={1}
                max={5}
                value={freeSlots}
                onChange={(e) => {
                  const v = Number(e.target.value);
                  if (Number.isInteger(v) && v >= 1 && v <= 5) setFreeSlots(v);
                }}
              />
            </label>
            <ObjectiveEditor value={objState} onChange={setObjState} lang={lang} />
            <button
              className="opt-run"
              disabled={session.busy || progress !== null || candidates.length === 0 || tooMany}
              onClick={run}
            >
              {progress ? tt('opt.running') : `${tt('opt.run')}（${combos} ${tt('opt.combos')}）`}
            </button>
          </div>
          {progress && (
            <OptimizerProgress
              done={progress.done}
              total={progress.total}
              onCancel={() => abortRef.current?.abort()}
              lang={lang}
            />
          )}
          {tooMany && <p className="opt-error">{tt('opt.tooMany')}</p>}
          {candidates.length === 0 && <p className="skills-hint">{tt('opt.needCandidates')}</p>}
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
