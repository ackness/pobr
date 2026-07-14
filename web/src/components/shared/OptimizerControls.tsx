/**
 * 寻优框架共享 UI 套件：目标/约束编辑器 + 进度条 + 结果表。
 * 宝石（Skills）/ 装备（Items）/ 天赋（Tree）三个寻优面板共用；
 * 结果表基于已评估的属性值即时重排——切换目标不重算。
 */

import type { VariantStats } from '../../api/types';
import { bindT, type Lang, type UiKey } from '../../lib/i18n';
import {
  CONSTRAINT_STATS,
  OBJECTIVE_PRESETS,
  feasibleOf,
  scoreOf,
  type Objective,
} from '../../lib/optimize';
import { STAT_SECTIONS } from '../../lib/statDisplay';
import { AppSelect } from './AppSelect';

/** 目标编辑器的受控状态（字符串态便于输入框直绑；objectiveOf 再解析）。 */
export interface ObjectiveState {
  preset: string;
  cStat: string;
  cMin: string;
  cMax: string;
}

export const DEFAULT_OBJECTIVE_STATE: ObjectiveState = {
  preset: 'dps',
  cStat: '',
  cMin: '',
  cMax: '',
};

/** 编辑态 → 打分目标。 */
export function objectiveOf(state: ObjectiveState): Objective {
  const preset = OBJECTIVE_PRESETS.find((p) => p.id === state.preset) ?? OBJECTIVE_PRESETS[0];
  const constraints =
    state.cStat && (state.cMin !== '' || state.cMax !== '')
      ? [
          {
            stat: state.cStat,
            ...(state.cMin !== '' ? { min: Number(state.cMin) } : {}),
            ...(state.cMax !== '' ? { max: Number(state.cMax) } : {}),
          },
        ]
      : [];
  return { stat: preset.stat, per: preset.per, constraints };
}

export function statLabel(id: string, lang: Lang): string {
  for (const section of STAT_SECTIONS) {
    const row = section.rows.find((r) => r.id === id);
    if (row) return row.label[lang];
  }
  return id;
}

function fmtNum(value: number): string {
  return value.toLocaleString('en-US', { maximumFractionDigits: 2 });
}

interface EditorProps {
  value: ObjectiveState;
  onChange: (next: ObjectiveState) => void;
  lang: Lang;
  disabled?: boolean;
}

/** 目标预设 + 单条约束（属性 / 下限 / 上限）。 */
export function ObjectiveEditor({ value, onChange, lang, disabled }: EditorProps) {
  const tt = bindT(lang);
  return (
    <>
      <label>
        {tt('opt.objective')}
        <AppSelect
          value={value.preset}
          options={OBJECTIVE_PRESETS.map((p) => ({ value: p.id, label: tt(p.labelKey as UiKey) }))}
          onChange={(preset) => onChange({ ...value, preset })}
          disabled={disabled}
          ariaLabel={tt('opt.objective')}
        />
      </label>
      <label>
        {tt('opt.constraint')}
        <AppSelect
          value={value.cStat}
          options={[
            { value: '', label: tt('opt.constraintNone') },
            ...CONSTRAINT_STATS.map((id) => ({ value: id, label: statLabel(id, lang) })),
          ]}
          onChange={(cStat) => onChange({ ...value, cStat })}
          disabled={disabled}
          ariaLabel={tt('opt.constraint')}
        />
      </label>
      {value.cStat && (
        <>
          <label>
            {tt('opt.min')}
            <input
              type="number"
              value={value.cMin}
              disabled={disabled}
              onChange={(e) => onChange({ ...value, cMin: e.target.value })}
            />
          </label>
          <label>
            {tt('opt.max')}
            <input
              type="number"
              value={value.cMax}
              disabled={disabled}
              onChange={(e) => onChange({ ...value, cMax: e.target.value })}
            />
          </label>
        </>
      )}
    </>
  );
}

interface ProgressProps {
  done: number;
  total: number;
  onCancel: () => void;
  lang: Lang;
}

/** 评估进度 + 取消（批间让出主线程时得以绘制/响应）。 */
export function OptimizerProgress({ done, total, onCancel, lang }: ProgressProps) {
  const tt = bindT(lang);
  return (
    <div className="opt-progress">
      <progress value={done} max={total} />
      <span>
        {done}/{total}
      </span>
      <button onClick={onCancel}>{tt('opt.cancel')}</button>
    </div>
  );
}

interface ResultsProps {
  baseline: Record<string, number>;
  /** 已排序结果（rankVariants 输出），由调用方截断。 */
  ranked: VariantStats[];
  objective: Objective;
  onApply: (variant: VariantStats) => void;
  lang: Lang;
  disabled?: boolean;
}

/** 结果表：基线行 + 逐变体（得分 / 相对基线 / 应用）。 */
export function OptimizerResults({
  baseline,
  ranked,
  objective,
  onApply,
  lang,
  disabled,
}: ResultsProps) {
  const tt = bindT(lang);
  const baseScore = scoreOf(baseline, objective);
  const deltaPct = (score: number): string =>
    baseScore > 0
      ? `${score >= baseScore ? '+' : ''}${(((score - baseScore) / baseScore) * 100).toFixed(1)}%`
      : '—';
  return (
    <table className="opt-table">
      <tbody>
        <tr className="opt-baseline">
          <td>{tt('opt.baseline')}</td>
          <td className="opt-score">{fmtNum(baseScore)}</td>
          <td>—</td>
          <td />
        </tr>
        {ranked.map((variant) => {
          if (variant.error) {
            return (
              <tr key={variant.index} className="is-infeasible">
                <td>{variant.label ?? `#${variant.index}`}</td>
                <td colSpan={2} className="opt-error-cell">
                  {variant.error}
                </td>
                <td />
              </tr>
            );
          }
          const feasible = feasibleOf(variant.stats, objective);
          const score = scoreOf(variant.stats, objective);
          return (
            <tr key={variant.index} className={feasible ? '' : 'is-infeasible'}>
              <td>
                {variant.label ?? `#${variant.index}`}
                {!feasible && <span className="opt-infeasible">（{tt('opt.infeasible')}）</span>}
              </td>
              <td className="opt-score">{fmtNum(score)}</td>
              <td className="opt-delta">{deltaPct(score)}</td>
              <td>
                <button disabled={disabled} onClick={() => onApply(variant)}>
                  {tt('opt.apply')}
                </button>
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
