import { calculatedSkillKey } from '../../app/calculatedSkill';
import { contributionLabel, removalChange } from '../../lib/sourceContributions';
import { COMMON_METRICS, defaultMetric } from '../../lib/metricSelection';
import { AppButton, AppInput } from '../shared/controls';
import { PageHeader } from '../shared/PageHeader';
import { formatApiError } from '../../api/error';
import { useMemo, useRef, useState } from 'react';
import type {
  AttributionResponse,
  MinionOutput,
  Breakdown,
  BreakdownMod,
  FormulaStep,
  HitDamageTable,
  HitMitigationRow,
  SkillDotRow,
  AilmentDetail,
  DefenceSlotRow,
  ReloadDetail,
  DisplayStatCategory,
  SkillMetricsResponse,
  CalculateBuildResponse,
} from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { formatNumber, statFormat, statMap, metricMeaning, formatStat } from '../../lib/statDisplay';
import { mainMinion } from '../../lib/mainMinion';
import {
  bindT,
  damageTypeLabel,
  originKindLabel,
  modTypeLabel,
  slotLabel,
  statCategoryLabel,
  statNameLabel as sharedStatNameLabel,
  type Lang,
} from '../../lib/i18n';
import { getBackend } from '../../api/backend';
import { isContinuation } from '../../lib/continuedLines';
import { groupModifiers, damageChain, resistanceIds, actionRateIsFinal, COMBINED_DPS_COMPONENTS, type ModifierMeasure } from '../../lib/calcsPresentation';
import { sourceGroupLabel } from '../../app/pages/calcs/ledger';
import { ModifierText } from './ModifierText';
import { useLocalizedLines } from '../../hooks/useLocalizedLines';
import { ConfigFieldLink } from '../config/ConfigFieldLink';
import { UnincludedModifiers } from '../shared/UnincludedModifiers';
import { useEffect } from 'react';
import { useSkillName, useSkillCatalog } from '../../hooks/useSkillName';
import { useLocalizedNames } from '../../hooks/useLocalizedLines';
import './calcs.css';
import { detailKind, detailFormula, detailInputLabel, detailInputValue, detailEmptyKey, StatDetailCache } from '../../lib/statDetails';
import { AppSelectControl } from '../shared/controls';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '../ui/tabs';
import { calcsLabel } from './calcsLabels';
import { SkillFormControls } from '../skills/SkillFormControls';
import { SkillMetricsTable } from './SkillMetricsTable';
import { useNavigateToPage } from '../../app/pages/navigate';
import { LedgerView } from '../../app/pages/calcs/LedgerView';
import { words, colon } from '../../app/text';
import { cleanLineText } from '../../lib/modifierText';
import { passiveNameRequest, usePassiveNodesBySkill } from '../tree/passiveNodeNames';

interface Props {
  session: BuildSession;
  lang: Lang;
  /** Display only; requests and mutations still use the current session. */
  settledCalc?: CalculateBuildResponse | null;
  /** 侧边栏点击跳转：要展开的 breakdown（对象引用每次点击新建，作为 effect 触发键）。 */
  focus?: { id: string } | null;
  /** 跳转消费完毕的回调（父级清空 focus，防止页签切回时重复触发滚动）。 */
  onFocusConsumed?: () => void;
}


function statNameLabel(lang: Lang, id: string): string {
  return id === 'TotalDPS' ? bindT(lang)('calcs.hitDps') : sharedStatNameLabel(lang, id);
}

/** How a formula step changes the running value, e.g. `+120`, `×1.35 (+35%)`, `≤ 75`. */
function formulaOperand(step: FormulaStep, stat?: string, lang: Lang = 'en-US', steps: readonly FormulaStep[] = []): string {
  const operandStat = step.op === 'cap' ? resistanceIds(stat ?? '')[1]
    : stat === 'TotalDPS' && step.op === 'chance_to_hit' ? 'HitChance'
    : stat === 'TotalDPS' && step.op === 'uses_per_second' && actionRateIsFinal(steps) ? 'ActionRate'
    : stat === 'CombinedDPS' ? COMBINED_DPS_COMPONENTS[step.op]
    : undefined;
  const n = operandStat ? formatStat(operandStat, step.operand, lang).replace(/%$/, '') : formatNumber(step.operand);
  const signed = step.operand < 0 ? `−${formatNumber(-step.operand)}` : `+${n}`;
  switch (step.op) {
    case 'base':
    case 'bonus_to_multiplier':
    case 'round':
    case 'accuracy_rating':
    case 'always_hits':
    case 'non_crit_hit':
    case 'average_hit':
    case 'skill_dot':
    case 'hit_dps':
    case 'ailment_source':
    case 'main_hand':
    case 'stronger_hand':
      return '';
    case 'off_hand_average':
      return `(M + ${n}) ÷ 2`;
    case 'off_hand_either':
      return `M + ${n} − M × ${n} ÷ 100`;
    case 'off_hand_alternating':
      return `2 ÷ (1 ÷ M + 1 ÷ ${n})`;
    case 'stronger_share':
      return `×${n}%`;
    case 'weaker_hand':
      return `+ ${n} × (1 − s)`;
    case 'crit_hits':
      return `${n}%`;
    case 'chance_to_hit':
    case 'per_second':
    case 'chance_to_apply':
      return `×${n}%`;
    case 'active_stacks':
    case 'ailment_effect':
    case 'ailment_rate':
    case 'enemy_mitigation':
    case 'uses_per_second':
    case 'dps_multiplier':
    case 'energy_trigger':
    case 'reload':
      return `×${n}`;
    case 'vs_evasion':
      return `A × 1.25 ÷ (A + ${n} × 0.3)`;
    case 'flat':
    case 'enemy_flat':
    case 'gear_adjusted':
    case 'gained':
    case 'slot_scaled':
    case 'flat_after':
    case 'poison_dps':
    case 'ignite_dps':
    case 'bleed_dps':
    case 'dot_dps':
    case 'off_hand_added':
      return signed;
    case 'increased':
    case 'enemy_increased':
    case 'inevitable_penalty':
    case 'bifurcate_bonus':
      return `×${formatNumber(1 + step.operand / 100)} (${signed}%)`;
    case 'hit_chance':
      return `×${n}%`;
    case 'lucky':
    case 'bifurcate':
      return '1 − (1 − p)²';
    case 'inevitable':
    case 'never_crit':
    case 'no_crit_bonus':
      return `= ${n}`;
    case 'enemy_increase_added':
      return `${signed}%`;
    case 'converted':
    case 'enemy_block':
      return `−${n}%`;
    case 'more':
    case 'action_speed':
      return `×${n}`;
    case 'added_time':
      return `${signed}s`;
    case 'hit_interval':
      return `${n}s`;
    case 'override':
      return `= ${n}`;
    case 'cap':
    case 'cooldown_cap':
    case 'server_tick_cap':
      return `≤ ${n}`;
    case 'floor':
      return `≥ ${n}`;
  }
}

/** A step can finish another stat before this formula finishes its own. */
function formulaResult(steps: readonly FormulaStep[], index: number, stat: string | undefined, lang: Lang): string {
  const next = steps[index + 1]?.op;
  const resultStat = stat === 'TotalDPS' && steps[index].op === 'average_hit' ? 'TotalHitAvg'
    : (next === 'cap' || next === 'floor') && resistanceIds(stat ?? '')[0]
      ? resistanceIds(stat!)[0]
      : index === steps.length - 1 ? stat ?? ''
      // The first Combined DPS step starts the sum, so its result is still that component.
      : stat === 'CombinedDPS' && index === 0 ? COMBINED_DPS_COMPONENTS[steps[0].op]
      : undefined;
  return resultStat !== undefined ? formatStat(resultStat, steps[index].result, lang) : formatNumber(steps[index].result);
}

/** The per-type table behind the average hit: base range, scaling, and both averages. */
function HitDamageView({ damage, lang }: { damage: HitDamageTable; lang: Lang }) {
  const tt = bindT(lang);
  const range = (min: number, max: number) => `${formatNumber(min)} – ${formatNumber(max)}`;
  const sum = (key: 'avg' | 'crit_avg') => damage.rows.reduce((total, row) => total + row[key], 0);
  // Without conversion or gain the base column already is the range before them.
  const moved = damage.sources.some((source) =>
    [...source.convert_mods, ...source.gain_mods].some((m) => m.active && (m.value ?? 0) > 0),
  );
  return (
    <div className="breakdown-scroll">
      <table className="breakdown-table hit-damage-table damage-type-table" aria-label={tt('calcs.damage.title')}>
        <caption>{tt('calcs.damage.title')}</caption>
        <thead>
          <tr>
            <th scope="col">{tt('calcs.damage.type')}</th>
            <th scope="col">{tt('calcs.damage.base')}</th>
            <th scope="col">{tt('calcs.damage.increased')}</th>
            <th scope="col">{tt('calcs.damage.more')}</th>
            <th scope="col">{tt('calcs.damage.hit')}</th>
            <th scope="col">{tt('calcs.damage.avg')}</th>
            <th scope="col">{tt('calcs.damage.critAvg')}</th>
          </tr>
        </thead>
        <tbody>
          {damage.rows.map((row) => (
            <tr key={row.damage_type} className={`damage-${row.damage_type.toLowerCase()}`}>
              <th scope="row">{damageTypeLabel(lang, row.damage_type)}</th>
              <td className="mod-value">{range(row.base_min, row.base_max)}</td>
              <td className="mod-value">{formatNumber(row.increased)}%</td>
              <td className="mod-value">
                ×{formatNumber(row.more)}
                {row.more_min !== 1 && ` · ${tt('calcs.damage.minOnly')} ×${formatNumber(row.more_min)}`}
                {row.more_max !== 1 && ` · ${tt('calcs.damage.maxOnly')} ×${formatNumber(row.more_max)}`}
              </td>
              <td className="mod-value">{range(row.min, row.max)}</td>
              <td className="mod-value">{formatNumber(row.avg)}</td>
              <td className="mod-value">{formatNumber(row.crit_avg)}</td>
            </tr>
          ))}
        </tbody>
        <tfoot>
          <tr>
            <th scope="row" colSpan={5}>{tt('calcs.damage.total')}</th>
            <td className="mod-value">{formatNumber(sum('avg'))}</td>
            <td className="mod-value">{formatNumber(sum('crit_avg'))}</td>
          </tr>
        </tfoot>
      </table>
      {damage.damage_effect !== 1 && (
        <p className="breakdown-note">{tt('calcs.damage.effect')}{colon(lang)}×{formatNumber(damage.damage_effect)}</p>
      )}
      {moved && (
        <>
          <table className="breakdown-table hit-damage-table damage-source-table" aria-label={tt('calcs.damage.sourceTitle')}>
            <caption>{tt('calcs.damage.sourceTitle')}</caption>
            <thead>
              <tr>
                <th scope="col">{tt('calcs.damage.type')}</th>
                <th scope="col">{tt('calcs.damage.sourceAdded')}</th>
                <th scope="col">{tt('calcs.damage.sourceEffect')}</th>
                <th scope="col">{tt('calcs.damage.sourceRange')}</th>
              </tr>
            </thead>
            <tbody>
              {damage.sources.map((source) => (
                <tr key={source.damage_type} className={`damage-${source.damage_type.toLowerCase()}`}>
                  <th scope="row">{damageTypeLabel(lang, source.damage_type)}</th>
                  <td className="mod-value">{range(source.added_min, source.added_max)}</td>
                  <td className="mod-value">×{formatNumber(source.added_effect)}</td>
                  <td className="mod-value">{range(source.min, source.max)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="breakdown-note">{tt('calcs.damage.sourceNote')}</p>
        </>
      )}
    </div>
  );
}

/** The lines behind each damage type's increased and more totals: shared ones first, then per type. */
function DamageLinesView({ damage, lang }: { damage: HitDamageTable; lang: Lang }) {
  const tt = bindT(lang);
  const typed = (type: string, kind: string, label: string, mods: BreakdownMod[]) => ({
    key: `${type}-${kind}`,
    label: `${damageTypeLabel(lang, type)} · ${label}`,
    mods,
  });
  const groups = [
    ...damage.sources.flatMap((source) => [
      typed(source.damage_type, 'added-min', tt('calcs.damage.addedMinLines'), source.added_min_mods),
      typed(source.damage_type, 'added-max', tt('calcs.damage.addedMaxLines'), source.added_max_mods),
      typed(source.damage_type, 'added-effect', tt('calcs.damage.addedEffectLines'), source.added_effect_mods),
      typed(source.damage_type, 'convert', tt('calcs.damage.convertLines'), source.convert_mods),
      typed(source.damage_type, 'gain', tt('calcs.damage.gainLines'), source.gain_mods),
    ]),
    { key: 'shared', label: tt('calcs.damage.sharedLines'), mods: damage.shared_mods },
    ...damage.rows.flatMap((row) => [
      { key: row.damage_type, label: `${damageTypeLabel(lang, row.damage_type)} · ${tt('calcs.damage.typeLines')}`, mods: row.mods },
      typed(row.damage_type, 'min', tt('calcs.damage.minLines'), row.min_mods),
      typed(row.damage_type, 'max', tt('calcs.damage.maxLines'), row.max_mods),
    ]),
  ].filter((group) => group.mods.length > 0);
  if (groups.length === 0) return null;
  return (
    <div className="damage-lines">
      {groups.map((group) => (
        <details key={group.key} className={`damage-lines-group damage-lines-${group.key.toLowerCase()}`}>
          <summary className="disclosure" data-disclosure-level="group">
            {group.label}
            <span className="breakdown-count">
              {group.mods.filter((m) => m.active).length} {tt('calcs.mods')}
            </span>
          </summary>
          <ModLines mods={group.mods} lang={lang} />
        </details>
      ))}
    </div>
  );
}

/** What the enemy does to each damage type of the average hit. */
function MitigationView({ rows, lang }: { rows: HitMitigationRow[]; lang: Lang }) {
  const tt = bindT(lang);
  const sum = (key: 'average' | 'mitigated') => rows.reduce((total, row) => total + row[key], 0);
  // Armour depends on the hit size, so the listed physical reduction is the non-critical one.
  const critsDiffer = rows.some(
    (row) => Math.abs(row.multiplier - row.taken * (1 - row.reduction / 100)) > 1e-6,
  );
  return (
    <div className="breakdown-scroll">
      <table className="breakdown-table hit-damage-table mitigation-table" aria-label={tt('calcs.mitigation.title')}>
        <caption>{tt('calcs.mitigation.title')}</caption>
        <thead>
          <tr>
            <th scope="col">{tt('calcs.damage.type')}</th>
            <th scope="col">{tt('calcs.mitigation.average')}</th>
            <th scope="col">{tt('calcs.mitigation.resist')}</th>
            <th scope="col">{tt('calcs.mitigation.reduction')}</th>
            <th scope="col">{tt('calcs.mitigation.taken')}</th>
            <th scope="col">{tt('calcs.mitigation.multiplier')}</th>
            <th scope="col">{tt('calcs.mitigation.after')}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.damage_type} className={`damage-${row.damage_type.toLowerCase()}`}>
              <th scope="row">{damageTypeLabel(lang, row.damage_type)}</th>
              <td className="mod-value">{formatNumber(row.average)}</td>
              <td className="mod-value">{row.resist === null ? tt('calcs.mitigation.armour') : `${formatNumber(row.resist)}%`}</td>
              <td className="mod-value">{formatNumber(row.reduction)}%</td>
              <td className="mod-value">×{formatNumber(row.taken)}</td>
              <td className="mod-value">×{formatNumber(row.multiplier)}</td>
              <td className="mod-value">{formatNumber(row.mitigated)}</td>
            </tr>
          ))}
        </tbody>
        <tfoot>
          <tr>
            <th scope="row">{tt('calcs.damage.total')}</th>
            <td className="mod-value">{formatNumber(sum('average'))}</td>
            <td colSpan={4} />
            <td className="mod-value">{formatNumber(sum('mitigated'))}</td>
          </tr>
        </tfoot>
      </table>
      {critsDiffer && <p className="breakdown-note">{tt('calcs.mitigation.critNote')}</p>}
      {rows
        .filter((row) => row.resist_inverted !== 0)
        .map((row) => (
          <p key={row.damage_type} className="breakdown-note">
            {tt('calcs.mitigation.invertedNote')
              .replace('{chance}', formatNumber(row.resist_inverted))
              .replace('{type}', damageTypeLabel(lang, row.damage_type))}
          </p>
        ))}
    </div>
  );
}

/** The enemy's own lines behind each row of the mitigation table. */
function EnemyLinesView({ rows, lang }: { rows: HitMitigationRow[]; lang: Lang }) {
  const tt = bindT(lang);
  const groups = rows
    .flatMap((row) => [
      {
        key: `${row.damage_type}-defence`,
        label: tt(row.resist === null ? 'calcs.mitigation.armourLines' : 'calcs.mitigation.resistLines'),
        type: row.damage_type,
        mods: row.defence_mods,
      },
      { key: `${row.damage_type}-taken`, label: tt('calcs.mitigation.takenLines'), type: row.damage_type, mods: row.taken_mods },
    ])
    .filter((group) => group.mods.length > 0);
  if (groups.length === 0) return null;
  return (
    <div className="damage-lines enemy-lines">
      {groups.map((group) => (
        <section key={group.key} className={`enemy-lines-section enemy-lines-${group.key.toLowerCase()}`}>
          <h5>
            {damageTypeLabel(lang, group.type)} · {group.label}
            <span className="breakdown-count">
              {group.mods.filter((m) => m.active).length} {tt('calcs.mods')}
            </span>
          </h5>
          <ModLines mods={group.mods} lang={lang} />
        </section>
      ))}
    </div>
  );
}

/** The skill's own damage over time per type, with the lines that scale each type. */
function SkillDotView({ rows, lang }: { rows: SkillDotRow[]; lang: Lang }) {
  const tt = bindT(lang);
  const showAura = rows.some((row) => row.aura !== 1);
  return (
    <>
      <div className="breakdown-scroll">
        <table className="breakdown-table hit-damage-table skill-dot-table" aria-label={tt('calcs.skillDot.title')}>
          <caption>{tt('calcs.skillDot.title')}</caption>
          <thead>
            <tr>
              <th scope="col">{tt('calcs.damage.type')}</th>
              <th scope="col">{tt('calcs.skillDot.base')}</th>
              <th scope="col">{tt('calcs.damage.increased')}</th>
              <th scope="col">{tt('calcs.damage.more')}</th>
              <th scope="col">{tt('calcs.skillDot.multiplier')}</th>
              {showAura && <th scope="col">{tt('calcs.skillDot.aura')}</th>}
              <th scope="col">{tt('calcs.skillDot.enemy')}</th>
              <th scope="col">{tt('calcs.skillDot.total')}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.damage_type} className={`damage-${row.damage_type.toLowerCase()}`}>
                <th scope="row">{damageTypeLabel(lang, row.damage_type)}</th>
                <td className="mod-value">{formatNumber(row.base)}</td>
                <td className="mod-value">{formatNumber(row.increased)}%</td>
                <td className="mod-value">×{formatNumber(row.more)}</td>
                <td className="mod-value">{formatNumber(row.dot_multiplier)}%</td>
                {showAura && <td className="mod-value">×{formatNumber(row.aura)}</td>}
                <td className="mod-value">×{formatNumber(row.enemy)}</td>
                <td className="mod-value">{formatNumber(row.total)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="damage-lines">
        {rows.filter((row) => row.mods.length > 0).map((row) => (
          <details key={row.damage_type} className={`damage-lines-group damage-lines-${row.damage_type.toLowerCase()}`}>
            <summary className="disclosure" data-disclosure-level="group">
              {damageTypeLabel(lang, row.damage_type)} · {tt('calcs.skillDot.lines')}
              <span className="breakdown-count">
                {row.mods.filter((m) => m.active).length} {tt('calcs.mods')}
              </span>
            </summary>
            <ModLines mods={row.mods} lang={lang} />
          </details>
        ))}
      </div>
    </>
  );
}

/** The hit a damaging ailment comes from and the estimate of how many instances are active. */
function AilmentView({ ailment, lang }: { ailment: AilmentDetail; lang: Lang }) {
  const tt = bindT(lang);
  type Key = 'sourceHit' | 'sourceCrit' | 'critShare' | 'hitChance' | 'applyChance' | 'baseDuration' | 'enemyDuration' | 'rate' | 'duration' | 'hitsPerSecond' | 'stacks' | 'maxStacks';
  const seconds = (value: number) => `${formatNumber(value)}${tt('calcs.ailment.seconds')}`;
  const rows: [Key, string][] = [
    ['sourceHit', formatNumber(ailment.source_hit)],
    ['sourceCrit', formatNumber(ailment.source_crit)],
    ['critShare', `${formatNumber(ailment.crit_share)}%`],
    ['hitChance', `${formatNumber(ailment.hit_chance)}%`],
    ['applyChance', `${formatNumber(ailment.apply_chance)}%`],
    ...(ailment.base_duration !== ailment.duration ? [['baseDuration', seconds(ailment.base_duration)] as [Key, string]] : []),
    ...(ailment.enemy_duration !== 1 ? [['enemyDuration', `×${formatNumber(ailment.enemy_duration)}`] as [Key, string]] : []),
    ...(ailment.rate !== 1 ? [['rate', `÷${formatNumber(ailment.rate)}`] as [Key, string]] : []),
    ['duration', seconds(ailment.duration)],
    ['hitsPerSecond', formatNumber(ailment.hits_per_second)],
    ...(ailment.stacks > 0 ? [['stacks', formatNumber(ailment.stacks)] as [Key, string]] : []),
    ['maxStacks', ailment.max_stacks === 0 ? '∞' : formatNumber(ailment.max_stacks)],
  ];
  const lineGroups = (
    [
      ['chance', ailment.chance_mods],
      ['duration', ailment.duration_mods],
      ['effect', ailment.effect_mods],
      ['rate', ailment.rate_mods],
    ] as const
  ).filter(([, mods]) => mods.length > 0);
  return (
    <>
      <div className="breakdown-scroll">
        {ailment.source.length > 1 && (
          <table className="breakdown-table hit-damage-table ailment-source-table" aria-label={tt('calcs.ailment.sourceTitle')}>
            <caption>{tt('calcs.ailment.sourceTitle')}</caption>
            <thead>
              <tr>
                <th scope="col">{tt('calcs.damage.type')}</th>
                <th scope="col">{tt('calcs.ailment.sourceHitShort')}</th>
                <th scope="col">{tt('calcs.ailment.sourceCritShort')}</th>
              </tr>
            </thead>
            <tbody>
              {ailment.source.map((row) => (
                <tr key={row.damage_type} className={`damage-${row.damage_type.toLowerCase()}`}>
                  <th scope="row">{damageTypeLabel(lang, row.damage_type)}</th>
                  <td className="mod-value">{formatNumber(row.hit)}</td>
                  <td className="mod-value">{formatNumber(row.crit)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        <table className="breakdown-formula ailment-table" aria-label={tt('calcs.ailment.title')}>
          <caption>{tt('calcs.ailment.title')}</caption>
          <tbody>
            {rows.map(([key, value]) => (
              <tr key={key} className={`ailment-${key}`}>
                <th scope="row">{tt(`calcs.ailment.${key}`)}</th>
                <td className="formula-result">{value}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {ailment.damage_roll !== 50 && (
          <p className="breakdown-note">
            {tt('calcs.ailment.rollNote').replace('{roll}', formatNumber(ailment.damage_roll))}
          </p>
        )}
        {ailment.stacks === 0 && <p className="breakdown-note">{tt('calcs.ailment.noRateNote')}</p>}
      </div>
      {lineGroups.length > 0 && (
        <div className="damage-lines ailment-lines">
          {lineGroups.map(([key, mods]) => (
            <details key={key} className={`damage-lines-group ailment-lines-${key}`}>
              <summary className="disclosure" data-disclosure-level="group">
                {tt(`calcs.ailment.${key}Lines`)}
                <span className="breakdown-count">
                  {mods.filter((m) => m.active).length} {tt('calcs.mods')}
                </span>
              </summary>
              <ModLines mods={mods} lang={lang} />
            </details>
          ))}
        </div>
      )}
    </>
  );
}

/** The crossbow cycle behind the `reload` step: a magazine fired, then the reload. */
/** The `slot_scaled` step of a defence, one row per item slot. */
function DefenceSlotsView({ slots, lang }: { slots: DefenceSlotRow[]; lang: Lang }) {
  const tt = bindT(lang);
  return (
    <div className="breakdown-scroll">
      <table className="breakdown-table hit-damage-table defence-slots-table" aria-label={tt('calcs.slots.title')}>
        <caption>{tt('calcs.slots.title')}</caption>
        <thead>
          <tr>
            <th scope="col">{tt('calcs.slots.slot')}</th>
            <th scope="col">{tt('calcs.slots.base')}</th>
            <th scope="col">{tt('calcs.slots.increased')}</th>
            <th scope="col">{tt('calcs.slots.more')}</th>
            <th scope="col">{tt('calcs.slots.extra')}</th>
          </tr>
        </thead>
        <tbody>
          {slots.map((row, index) => (
            <tr key={`${row.slot}-${index}`}>
              <th scope="row">{slotLabel(lang, row.slot)}</th>
              <td className="mod-value">{formatNumber(row.base)}</td>
              <td className="mod-value">{formatNumber(row.increased)}%</td>
              <td className="mod-value">×{formatNumber(row.more)}</td>
              <td className="mod-value">{formatNumber(row.extra)}</td>
            </tr>
          ))}
        </tbody>
        <tfoot>
          <tr>
            <th scope="row" colSpan={4}>{tt('calcs.damage.total')}</th>
            <td className="mod-value">{formatNumber(slots.reduce((total, row) => total + row.extra, 0))}</td>
          </tr>
        </tfoot>
      </table>
    </div>
  );
}

function ReloadView({ reload, lang }: { reload: ReloadDetail; lang: Lang }) {
  const tt = bindT(lang);
  const seconds = (value: number) => `${formatNumber(value)}${tt('calcs.ailment.seconds')}`;
  type Key = 'firingRate' | 'bolts' | 'firingTime' | 'reloadTime' | 'rate';
  const rows: [Key, string][] = [
    ['firingRate', formatNumber(reload.firing_rate)],
    ['bolts', reload.bolts === null ? '∞' : formatNumber(reload.bolts)],
    ['firingTime', seconds(reload.firing_time)],
    ['reloadTime', seconds(reload.reload_time)],
    ['rate', formatNumber(reload.rate)],
  ];
  return (
    <div className="breakdown-scroll">
      <table className="breakdown-formula reload-table" aria-label={tt('calcs.reload.title')}>
        <caption>{tt('calcs.reload.title')}</caption>
        <tbody>
          {rows.map(([key, value]) => (
            <tr key={key} className={`reload-${key}`}>
              <th scope="row">{tt(`calcs.reload.${key}`)}</th>
              <td className="formula-result">{value}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** Modifier lines with type, applied value, text and source; lines the calculation skips are dimmed. */
export function ModLines({ mods, lang, measure }: { mods: BreakdownMod[]; lang: Lang; measure?: ModifierMeasure }) {
  const tt = bindT(lang);
  return <div className="modifier-groups">{groupModifiers(mods, measure).map(group => <details className="modifier-source-group" key={group.source}>
    <summary className="disclosure" data-disclosure-level="group"><span className="modifier-source-heading"><strong>{sourceGroupLabel(lang, group.source)}</strong> <span>({group.mods.length})</span></span>
      {group.subtotals.length > 0 && <span className="modifier-subtotals">{tt('calcs.sourceTotals')}{colon(lang)}{group.subtotals.map(total => <span key={total.type}>
        {modTypeLabel(lang, total.type)} {total.type === 'MORE' ? '×' : ''}{formatNumber(total.value)}{total.type === 'INC' || (total.type === 'BASE' && measure?.unit === 'percent') ? '%' : ''}
      </span>)}</span>}
    </summary>
    <ModifierRows mods={group.mods} lang={lang} />
  </details>)}</div>;
}

function isNamelessItemMod(mod: BreakdownMod): boolean {
  return mod.origin_kind === 'Item' && !/[0-9%:]/.test(mod.source_text ?? '');
}

function ModifierRows({ mods, lang }: { mods: BreakdownMod[]; lang: Lang }) {
  const tt = bindT(lang);
  const raw = useMemo(() => mods.map(mod => cleanLineText(mod.source_text ?? '')), [mods]);
  const translated = useLocalizedLines(raw, lang);
  const nodes = usePassiveNodesBySkill(mods.some(mod => mod.origin_kind === 'PassiveNode' || mod.origin_kind === 'AscendancyNode'));
  const nameRequests = useMemo(() => mods.map(mod => {
    const node = nodes.get((mod.origin_id ?? '').replace(/^node\./, ''));
    return (node ? passiveNameRequest(node)[0] : undefined)
      ?? { kind: 'base' as const, name: !node && isNamelessItemMod(mod) ? mod.source_text ?? '' : '' };
  }), [mods, nodes]);
  const names = useLocalizedNames(nameRequests, lang);
  const source = (mod: BreakdownMod, index: number) => {
    if (mod.origin_id === 'campaign.resistance_penalty') return words(lang, 'Campaign resistance penalty', '剧情抗性惩罚', '劇情抗性懲罰');
    const label = mod.origin_kind ? originKindLabel(lang, mod.origin_kind) : sourceGroupLabel(lang, 'other');
    return [label, names[index], mod.slot ? slotLabel(lang, mod.slot) : null].filter(Boolean).join(' · ');
  };
  return <ul className="modifier-rows">{mods.map((mod, index) => {
    if (isContinuation(raw[index], translated[index])) return null;
    return <li key={index} className={mod.active ? undefined : 'is-inactive'}>
      <div className="modifier-row-head"><span className={`mod-type mod-type-${mod.mod_type.toLowerCase()}`}>{modTypeLabel(lang, mod.mod_type)}</span><strong className="mod-value">{mod.value === null ? '—' : formatNumber(mod.value)}</strong></div>
      <div className="mod-text">{!mod.source_text || isNamelessItemMod(mod)
        ? <>{tt(mod.origin_kind === 'Item' ? 'calcs.missingModifier' : mod.origin_kind === 'CharacterBase' ? 'calcs.baseFrom.CharacterBase' : mod.origin_kind === 'GameConstant' ? 'calcs.baseFrom.GameConstant' : mod.origin_kind === 'SupportGem' ? 'calcs.baseFrom.SupportGem' : 'calcs.baseDerived')}
          {mod.source_text && <details><summary className="disclosure">{tt('calcs.details')}</summary><pre>{mod.source_text}</pre></details>}</>
        : <ModifierText raw={mod.source_text} translated={translated[index]} lang={lang} />}
        {!mod.active && <> <span className="mod-inactive-tag">{tt('calcs.inactive')}</span></>}</div>
      <p className="mod-origin">{source(mod, index)}{lang !== 'en-US' && /[A-Za-z]{3}/.test(names[index] ?? '') && <small className="modifier-fallback">{tt('calcs.untranslated')}</small>}</p>
    </li>;
  })}</ul>;
}

function FormulaSummary({ breakdown, stat, lang }: { breakdown: Breakdown; stat?: string; lang: Lang }) {
  const tt = bindT(lang);
  const formula = breakdown.formula;
  if (!formula?.steps.length) return null;
  // Use returned steps only. Special operations keep their actual intermediate result visible.
  const summary = formula.steps.reduce((expression, step, index) => {
    const value = formulaResult(formula.steps, index, stat, lang);
    if (index === 0) return value;
    if (['flat', 'gained', 'flat_after', 'enemy_flat', 'off_hand_added', 'dot_dps', 'poison_dps', 'ignite_dps', 'bleed_dps'].includes(step.op)) return `(${expression} ${formulaOperand(step, stat, lang, formula.steps)})`;
    if (['increased', 'more', 'action_speed', 'uses_per_second', 'enemy_mitigation', 'chance_to_hit', 'dps_multiplier', 'energy_trigger', 'reload'].includes(step.op)) return `(${expression} ${formulaOperand(step, stat, lang, formula.steps).replace(/ \(.*\)$/, '')})`;
    if (step.op === 'converted') return `(${expression} ×${formatNumber(1 - step.operand / 100)})`;
    return `${expression} → ${tt(`calcs.step.${step.op}`)} ${value}`;
  }, '');
  const converted = formula.steps.some(step => step.op === 'converted' && step.operand !== 0);
  const targets = breakdown.sources?.filter(group => group.base_total !== 0 && /ConvertTo/.test(group.name)).map(group => statNameLabel(lang, group.name));
  return <div className="formula-conclusion"><p>{stat ? statNameLabel(lang, stat) + colon(lang) : ''}<span>{summary.split(/(\s+)/).map((part, index) => /\d/.test(part) ? <span className="formula-number" key={index}>{part}</span> : part)}</span> = <strong>{formatStat(stat ?? '', formula.value, lang)}{stat?.endsWith('DPS') ? ' DPS' : stat === 'Life' || stat === 'EnergyShield' ? words(lang, ' points', ' 点', ' 點') : ''}</strong></p>
    {converted && <p className="breakdown-note">{targets?.length ? targets.join(lang === 'en-US' ? ', ' : '、') : tt('calcs.conversionUnknown')}</p>}
  </div>;
}

function DamageChainView({ breakdown, lang }: { breakdown: Breakdown; lang: Lang }) {
  const tt = bindT(lang);
  const chain = damageChain(breakdown);
  const missing = chain.filter(link => link.value === undefined);
  return <section className="damage-chain"><h4>{tt('calcs.damageChain')}</h4><ol>{chain.filter(link => link.value !== undefined).map(link => <li key={link.key}>
    <span>{tt(link.key)}</span><strong>{link.unit === 'multiplier' ? '×' : ''}{link.stat ? formatStat(link.stat, link.value!, lang) : formatNumber(link.value!)}{link.unit === 'rate' ? words(lang, ' /s', ' 次/秒', ' 次/秒') : link.unit === 'percent' && !link.stat ? '%' : link.unit === 'dps' ? ' DPS' : ''}</strong>
  </li>)}</ol>
    {missing.length > 0 && <p className="breakdown-note">{tt('calcs.chainMissing').replace('{links}', missing.map(link => tt(link.key)).join(lang === 'en-US' ? ', ' : '、'))}</p>}
  </section>;
}

function ResistanceView({ breakdown, stat, lang, values, related, onSelect }: { breakdown: Breakdown; stat: string; lang: Lang;
  values?: ReadonlyMap<string, number | null>; related?: Record<string, Breakdown>; onSelect?: (stat: string) => void }) {
  const tt = bindT(lang);
  const ids = resistanceIds(stat);
  if (!ids.length) return null;
  const steps = breakdown.formula?.steps ?? [];
  const cap = steps.find(step => step.op === 'cap');
  const before = cap ? steps[steps.indexOf(cap) - 1]?.result : undefined;
  const fallback = [before, cap?.operand, undefined];
  const labels = ['calcs.resistTotal', 'calcs.resistCap', 'calcs.resistOver'] as const;
  const source = (id: string) => related?.[id] ?? (id === stat + 'Total' ? breakdown : undefined);
  return <section className="resistance-detail"><div className="resistance-summary">{ids.map((id, index) => {
    const value = values?.get(id) ?? related?.[id]?.formula?.value ?? fallback[index];
    return <section key={id}><h4>{tt(labels[index])}</h4><strong className="mod-value">{value == null ? tt('calcs.missingValue') : formatStat(id, value, lang)}</strong></section>;
  })}</div>
    {ids.map((id, index) => <details className="resistance-sources" key={id}><summary className="disclosure" data-disclosure-level="group">{tt(labels[index])}{colon(lang)}{tt('calcs.source')}</summary>
      {onSelect && <AppButton onClick={() => onSelect(id)}>{statNameLabel(lang, id)}</AppButton>}
      {source(id)?.mods.length ? <ModLines mods={source(id)!.mods} lang={lang} />
        : source(id)?.sources?.some(group => group.mods.length) ? source(id)!.sources!.filter(group => group.mods.length).map(group => <section key={group.name}><h5>{statNameLabel(lang, group.name)}</h5><ModLines mods={group.mods} lang={lang} measure={{ stat: group.name, unit: statFormat(group.name) }} /></section>)
        : source(id)?.derived ? <p className="formula-conclusion">{detailFormula(source(id)!.derived!.formula, source(id)!.derived!.inputs, lang, true)}</p>
        : <p className="breakdown-note">{tt('calcs.missingSources')}</p>}
    </details>)}
  </section>;

}

export function BreakdownTable({ breakdown, lang, stat, unboundedStacks = false, nested = false, values, related, onSelect, modifierMeasure }: { breakdown: Breakdown; lang: Lang; stat?: string; unboundedStacks?: boolean; nested?: boolean; values?: ReadonlyMap<string, number | null>; related?: Record<string, Breakdown>; onSelect?: (stat: string) => void; modifierMeasure?: ModifierMeasure }) {
  const tt = bindT(lang);
  if (breakdown.derived) {
    const derived = breakdown.derived;
    const sourceHints = derived.context.filter(input => input.id.startsWith('EnergyNeeds'));
    const context = derived.context.filter(input => !input.id.startsWith('EnergyNeeds'));
    const inputValue = (input: { id: string; value: number | null }) =>
      unboundedStacks && input.id === 'MaximumAilmentStacks' && input.value === 0
        ? '∞' : detailInputValue(input, lang);
    return <div className="breakdown-detail breakdown-derived">
      <h4 className="breakdown-heading">{calcsLabel(lang, 'derived')}</h4>
      <p className="breakdown-note">{tt('calcs.derivedHint')}</p>
      {sourceHints.map(hint => <p className="breakdown-note" key={hint.id}>{statNameLabel(lang, hint.id)}</p>)}
      <p className="formula-conclusion">{detailFormula(derived.formula, derived.inputs, lang, true)}{stat && values?.has(stat) ? ` = ${formatStat(stat, values.get(stat) ?? null, lang)}` : ''}</p>
      <details className="formula-steps"><summary className="disclosure">{tt('calcs.formula')}</summary><p className="derived-formula">{detailFormula(derived.formula, derived.inputs, lang)}</p></details>
      <table className="breakdown-table"><caption>{tt('calcs.inputs')}</caption><tbody>
        {derived.inputs.map(input => <tr key={input.id}><th scope="row">{detailInputLabel(input.id, derived.inputs, lang)}</th><td>{inputValue(input)}</td></tr>)}
      </tbody></table>
      {context.length > 0 && <table className="breakdown-table"><caption>{tt('calcs.context')}</caption><tbody>
        {context.map(input => <tr key={input.id}><th scope="row">{statNameLabel(lang, input.id)}</th><td>{inputValue(input)}</td></tr>)}
      </tbody></table>}
    </div>;
  }
  if (breakdown.kind === 'sources' && breakdown.actor?.kind !== 'minion') {
    const groups = breakdown.sources?.filter(group => group.mods.length > 0) ?? [];
    return <div className="breakdown-detail breakdown-sources">
      <h4 className="breakdown-heading">{calcsLabel(lang, 'sources')}</h4>
      <p className="breakdown-note">{tt('calcs.sourcesNote')}</p>
      {groups.length > 0 && <p className="breakdown-note breakdown-totals-note">{tt('calcs.totalsNote')}</p>}
      {groups.length === 0 && <p>{tt('calcs.noSources')}</p>}
      {groups.map(group => <section key={group.name}>
        <h5>{statNameLabel(lang, group.name)}</h5>
        {group.mods.some(mod => mod.active && mod.mod_type === 'MORE') && <p className="breakdown-note">{tt(group.more_precision === null ? 'calcs.moreRounded' : 'calcs.moreFloored')} {group.more_precision ?? 2}</p>}
        <BreakdownTable breakdown={{ ...group, formula: null }} lang={lang} nested modifierMeasure={{ stat: group.name, unit: statFormat(group.name) }} />
      </section>)}
    </div>;
  }
  const damageLines = [
    ...(breakdown.damage ? [breakdown.damage.shared_mods, ...breakdown.damage.rows.map((row) => row.mods)] : []),
    ...(breakdown.skill_dot ?? []).map((row) => row.mods),
  ].flat();
  const hands = breakdown.hands ?? [];
  // Each hand lists the lines its own pass counts; the stat's own list would repeat the main hand's.
  const linesPerHand = hands.some((hand) => hand.mods.length > 0);
  return (
    <div className="breakdown-detail">
      {stat && <ResistanceView breakdown={breakdown} stat={stat} lang={lang} values={values} related={related} onSelect={onSelect} />}
      <FormulaSummary breakdown={breakdown} stat={stat} lang={lang} />
      {stat === 'CombinedDPS' && onSelect && <div className="combined-components">{breakdown.formula?.steps.map((step, index) => {
        const component = COMBINED_DPS_COMPONENTS[step.op];
        return component ? <AppButton key={index} onClick={() => onSelect(component)}>{statNameLabel(lang, component)} {formatStat(component, step.operand, lang)} DPS</AppButton> : null;
      })}</div>}
      {stat === 'TotalDPS' && hands.length === 0 && <DamageChainView breakdown={breakdown} lang={lang} />}
      {breakdown.damage && <HitDamageView damage={breakdown.damage} lang={lang} />}
      {breakdown.skill_dot && <SkillDotView rows={breakdown.skill_dot} lang={lang} />}
      {breakdown.formula && breakdown.formula.steps.length > 0 ? (
        <>
          <h4 className="breakdown-heading">{calcsLabel(lang, 'steps')}</h4>
          <p className="breakdown-note">{tt('calcs.stepsHint')}</p>
          <details className="formula-steps"><summary className="disclosure">{tt('calcs.formula')}</summary>
          <div className="breakdown-scroll"><table className="breakdown-formula" aria-label={tt('calcs.formula')}>
            <tbody>
              {breakdown.formula.steps.map((step, i) => (
                <tr key={i} className={`formula-step formula-${step.op}`}>
                  <th scope="row">{tt(`calcs.step.${step.op}`)}</th>
                  <td className="formula-operand">{formulaOperand(step, stat, lang, breakdown.formula!.steps)}</td>
                  <td className="formula-result">{formulaResult(breakdown.formula!.steps, i, stat, lang)}</td>
                </tr>
              ))}
              <tr className="formula-final">
                <th scope="row">{tt('calcs.formulaResult')}</th>
                <td />
                <td className="formula-result">{formatStat(stat ?? '', breakdown.formula.value, lang)}</td>
              </tr>
            </tbody>
          </table></div></details>
        </>
      ) : (
        <>
          <div className="breakdown-summary">
            <span>
              {tt('calcs.baseTotal')}{colon(lang)}<strong>{formatNumber(breakdown.base_total)}</strong>
            </span>
            <span>
              {tt('calcs.incTotal')}{colon(lang)}<strong>{formatNumber(breakdown.inc_total)}%</strong>
            </span>
            {breakdown.more_total !== 1 && (
              <span>
                {tt('calcs.moreTotal')}{colon(lang)}<strong>×{formatNumber(breakdown.more_total)}</strong>
              </span>
            )}
          </div>
          {!nested && <p className="breakdown-note breakdown-totals-note">{tt('calcs.totalsNote')}</p>}
        </>
      )}
      {breakdown.mitigation && breakdown.mitigation.length > 0 && (
        <>
          <div className="mitigation-context"><MitigationView rows={breakdown.mitigation} lang={lang} />
            <aside><h4>{tt('calcs.enemyConditions')}</h4><ul>{breakdown.mitigation.map(row => <li key={row.damage_type}>
              <ConfigFieldLink field={row.resist === null ? 'enemyPhysicalReduction' : `enemy${row.damage_type}Resist`} lang={lang}>
                {damageTypeLabel(lang, row.damage_type)}{colon(lang)}{formatNumber(row.resist ?? row.reduction)}% {tt(row.resist === null ? 'calcs.mitigation.reduction' : 'calcs.mitigation.resist')}
              </ConfigFieldLink>
            </li>)}</ul></aside>
          </div>
          <details className="enemy-modifiers"><summary className="disclosure">{tt('calcs.modifier')} ({breakdown.mitigation.reduce((sum, row) => sum + row.defence_mods.length + row.taken_mods.length, 0)})</summary><EnemyLinesView rows={breakdown.mitigation} lang={lang} /></details>
        </>
      )}
      {breakdown.ailment && <AilmentView ailment={breakdown.ailment} lang={lang} />}
      {breakdown.reload && <ReloadView reload={breakdown.reload} lang={lang} />}
      {breakdown.slots && breakdown.slots.length > 0 && <DefenceSlotsView slots={breakdown.slots} lang={lang} />}
      {hands.length > 0 && <p className="breakdown-note breakdown-hands-note">{tt('calcs.hand.note')}</p>}
      {hands.map((hand) => (
        <section key={hand.hand} className={`breakdown-hand breakdown-hand-${hand.hand}`} aria-label={tt(`calcs.hand.${hand.hand}`)}>
          <h4 className="breakdown-hand-title">{tt(`calcs.hand.${hand.hand}`)}</h4>
          <BreakdownTable breakdown={hand} lang={lang} stat={stat} nested onSelect={onSelect} />
        </section>
      ))}
      {!linesPerHand && [...breakdown.mods, ...damageLines].some((m) => !m.active) && (
        <p className="breakdown-note">{tt('calcs.inactiveNote')}</p>
      )}
      {breakdown.damage && <DamageLinesView damage={breakdown.damage} lang={lang} />}
      {!linesPerHand && breakdown.mods.length > 0 && <ModLines mods={breakdown.mods} lang={lang} measure={modifierMeasure} />}
    </div>
  );
}

export function FullDpsView({ calc, lang }: { calc: CalculateBuildResponse | null; lang: Lang }) {
  const tt = bindT(lang);
  const skillName = useSkillName(lang);
  const full = calc?.full_dps;
  const goTo = useNavigateToPage();
  return <section className="full-dps-view" aria-labelledby="full-dps-heading">
    <h3 id="full-dps-heading">{calcsLabel(lang, 'fullDps')}</h3>
    {full && full.skills.length > 0 ? <>
      <p className="calcs-hint">{tt('skills.fullDpsHint')}</p>
      <div className="breakdown-summary">
        <span>{statNameLabel(lang, 'FullDPS')}{colon(lang)}<strong>{formatStat('FullDPS', full.dps, lang)}</strong></span>
        <span>{statNameLabel(lang, 'FullDotDPS')}{colon(lang)}<strong>{formatStat('FullDotDPS', full.dot_dps, lang)}</strong></span>
      </div>
      <div className="breakdown-scroll"><table className="breakdown-table full-dps-table">
        <caption>{tt('calcs.fullDpsContribution')}</caption>
        <thead><tr><th>{tt('calcs.skill')}</th><th>{tt('skills.count')}</th><th>{tt('calcs.fullDpsContribution')}</th><th>{tt('calcs.diagnostics')}</th></tr></thead>
        <tbody>{full.skills.map(row => <tr key={`${row.group_index}-${row.active_skill_index}`}>
          <th>{tt('calcs.group')} {row.group_index + 1} · {skillName(row.skill_id)}</th><td>{formatNumber(row.count)}</td>
          <td>{row.error ? '—' : formatStat('FullDPS', row.dps, lang)}</td><td>{row.error}{row.unsupported_modifiers.length > 0 && <details><summary className="disclosure">{tt('calcs.unsupported')} ({row.unsupported_modifiers.length})</summary>
            <UnincludedModifiers lines={row.unsupported_modifiers} lang={lang} /></details>}</td>
        </tr>)}</tbody>
      </table></div>
    </> : <>
      <p role="status" className="calcs-hint">{calcsLabel(lang, 'fullDpsEmpty')}</p>
      <AppButton onClick={() => goTo('skills')}>{tt('calcs.goToSkills')}</AppButton>
    </>}
  </section>;
}

/** Every active effect is calculated independently; selection only changes inspection context. */
function SkillMetricsView({ session, lang }: { session: BuildSession; lang: Lang }) {
  const tt = bindT(lang);
  const [completed, setCompleted] = useState<{ version: number; report: SkillMetricsResponse } | null>(null);
  const [running, setRunning] = useState(false);
  const [failure, setFailure] = useState<{ version: number; message: string } | null>(null);
  const { stateVersion, runSkillMetrics } = session;
  const report = completed?.version === stateVersion ? completed.report : null;
  const displayed = completed?.report;
  const error = failure?.version === stateVersion ? failure.message : null;
  useEffect(() => {
    let cancelled = false;
    setRunning(true);
    const timer = setTimeout(() => {
      setFailure(null);
      runSkillMetrics()
        .then((result) => {
          if (!cancelled) setCompleted({ version: stateVersion, report: result });
        })
        .catch((err) => {
          if (!cancelled) setFailure({ version: stateVersion, message: formatApiError(err) });
        })
        .finally(() => {
          if (!cancelled) setRunning(false);
        });
    }, 250);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [stateVersion, runSkillMetrics]);

  const catalog = useSkillCatalog();
  const mainIndex = session.calcParams.main_socket_group;

  return (
    <section className="ui-card attribution-view" aria-labelledby="skill-metrics-heading" aria-busy={running}>
      <h3 id="skill-metrics-heading" tabIndex={-1}>{tt('calcs.skillMetrics')}</h3>
      <p className="calcs-hint">{tt('calcs.skillMetricsHint')}</p>
      {mainIndex !== undefined && <SkillFormControls session={session} lang={lang} groupIndex={mainIndex} catalog={catalog} />}
      {running && <p className="calcs-hint" role="status">{tt('calcs.running')}</p>}
      {error && <div className="calc-error" role="alert">
        <p>{tt('calcs.overviewFailed')}</p>
        <details><summary className="disclosure">{tt('calcs.errorDetails')}</summary><p>{error}</p></details>
      </div>}
      {report?.item_errors.map((issue, index) => (
        <div className="calc-error" key={`${issue.slot}-${index}`}>
          <p>{slotLabel(lang, issue.slot)}{colon(lang)}{tt('calcs.itemExcluded')}</p>
          <details><summary className="disclosure">{tt('calcs.errorDetails')}</summary><p>{issue.message}</p></details>
        </div>
      ))}
      {displayed && (displayed.skills.length === 0 ? (
        <p className="calcs-hint">{tt('calcs.skillMetricsEmpty')}</p>
      ) : (
        <SkillMetricsTable session={session} lang={lang} report={report} displayed={displayed} />
      ))}
    </section>
  );
}

export function AttributionView({ session, lang }: { session: BuildSession; lang: Lang }) {
  const tt = bindT(lang);
  const selection = calculatedSkillKey(session);
  const [completed, setCompleted] = useState<{ version: number; selection: string; report: AttributionResponse } | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fields = ['CombinedDPS', 'Life', 'EnergyShield', 'TotalEHP'];
  const sequence = useRef(0);
  const report = completed?.version === session.stateVersion ? completed.report : null;
  useEffect(() => {
    sequence.current++;
    setRunning(false);
    setError(null);
    return () => { sequence.current++; };
  }, [session.stateVersion]);

  const run = async () => {
    if (session.busy || !session.calc) return;
    const current = ++sequence.current;
    const version = session.stateVersion;
    setRunning(true);
    setError(null);
    try {
      const result = await session.runAttribution(fields);
      if (current === sequence.current) setCompleted({ version, selection, report: result });
    } catch (err) {
      if (current === sequence.current) setError(formatApiError(err));
    } finally {
      if (current === sequence.current) setRunning(false);
    }
  };

  const skillName = useSkillName(lang);
  const label = (kind: string, id: string) => {
    if (kind === 'socket_group') {
      const group = session.socketGroups[Number(id)];
      const gems = group?.gems.map(gem =>
        gem.skill_id ? skillName(gem.skill_id) : gem.name_spec || tt('skills.unresolved')).join(' + ');
      return `${contributionLabel(lang, kind, id)}${gems ? ` · ${gems}` : ''}`;
    }
    return contributionLabel(lang, kind, id);
  };

  return (
    <section className="ui-card attribution-view" aria-labelledby="attribution-heading">
      <h3 id="attribution-heading" tabIndex={-1}>{tt('calcs.attribution')}</h3>
      <p className="calcs-hint">{tt('minion.player')}{session.calc?.main_skill && ` · ${skillName(session.calc.main_skill.skill_id)}`}</p>
      <p className="calcs-hint">
{tt('calcs.attributionHint')}
      </p>
      <AppButton onClick={run} disabled={running || session.busy || !session.calc}>
        {running ? tt('calcs.running') : tt('calcs.runAttribution')}
      </AppButton>
      {completed && !report && <p className="calcs-hint" role="status">{tt(completed.selection !== selection ? 'overview.skillAnalysisChanged' : 'calcs.attributionStale')}</p>}
      {error && <div className="calc-error">{error}</div>}
      {report && (
        <div className="breakdown-scroll">
          <table className="breakdown-table attribution-table">
            <thead>
              <tr>
                <th>{tt('calcs.source')}</th>
                {fields.map((f) => (
                  <th key={f}>{statNameLabel(lang, f)}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              <tr className="attribution-baseline">
                <td>{tt('calcs.baseline')}</td>
                {fields.map((f) => (
                  <td key={f}>{formatStat(f, report.baseline[f] ?? null, lang)}</td>
                ))}
              </tr>
              {[...report.entries]
                .sort(
                  (a, b) => Math.abs(b.deltas[fields[0]] ?? 0) - Math.abs(a.deltas[fields[0]] ?? 0),
                )
                .map((entry) => (
                  <tr key={`${entry.kind}-${entry.id}`}>
                    <td>
                      {label(entry.kind, entry.id)}
                    </td>
                    {fields.map((f) => {
                      const delta = removalChange(entry.deltas[f] ?? 0);
                      const baseline = report.baseline[f] ?? 0;
                      const pct = baseline !== 0 ? (delta / baseline) * 100 : 0;
                      return (
                        <td key={f} className={delta > 0 ? 'delta-pos' : delta < 0 ? 'delta-neg' : ''}>
                          {delta === 0 ? '—' : `${delta > 0 ? '+' : '-'}${formatStat(f, Math.abs(delta), lang)}`}
                          {Math.abs(pct) >= 0.05 && <span className="delta-pct">{lang === 'en-US' ? ' (' : '（'}{formatNumber(pct, 1, 1)}%{lang === 'en-US' ? ')' : '）'}</span>}
                        </td>
                      );
                    })}
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

/** Calcs 页：字段点击展开 breakdown（消费 0.3）+ 归因视图（消费 0.4）。 */
/** One actor in the picker: a minion's index alone can repeat across recalculations of a changed build. */
function actorKey(minion: MinionOutput) {
  return `${minion.index}:${minion.minion_id}:${minion.summoned_by}`;
}

/** A summoner build opens on the minion its calculated skill summons; arriving from one of the player's stats opens on the player. */
/**
 * The actor Calcs follows: the calculated skill's minion, else the player. Null without a result, so
 * minion → failed calculation → player skill still changes the key and moves the view to the player.
 */
export function followedActorKey(calc: CalculateBuildResponse | null): string | null {
  if (!calc) return null;
  const main = mainMinion(calc);
  return main ? actorKey(main) : 'player';
}

function openingMinion(calc: CalculateBuildResponse | null, focus: Props['focus']): MinionOutput | undefined {
  if (focus && focus.id !== 'skill-metrics' && !/^Minion:/.test(focus.id)) return undefined;
  return mainMinion(calc);
}

export function CalcsPanel({ session, lang, settledCalc, focus, onFocusConsumed }: Props) {
  const tt = bindT(lang);
  const skillName = useSkillName(lang);
  const [view, setView] = useState('metrics');
  const [category, setCategory] = useState('all');
  const [commonOnly, setCommonOnly] = useState(true);
  const calc = session.calcError ? null : session.calc ?? settledCalc ?? null;
  const [opening] = useState(() => openingMinion(calc, focus));
  const [open, setOpen] = useState<string | null>(() => opening
    ? defaultMetric(Object.entries(opening.stats).map(([id, value]) => ({ id, value })), true)
    : defaultMetric(calc?.stats ?? [], !!calc?.main_skill));
  const [actor, setActor] = useState(() => opening ? actorKey(opening) : 'player');
  const detailHeading = useRef<HTMLHeadingElement>(null);
  const focusMinionDetail = useRef(false);
  const selectedMinion = calc?.minions?.find(minion => actorKey(minion) === actor);
  // Minion names by game variety id; English without a MonsterVarieties.Name record.
  const minionNames = useLocalizedNames(useMemo(() => (calc?.minions ?? []).map(minion =>
    ({ kind: 'monster' as const, id: minion.monster_variety_id, name: minion.name })), [calc?.minions]), lang);
  const minionName = (minion: MinionOutput) => minionNames[minion.index] ?? minion.name;
  const actorBreakdowns = selectedMinion?.breakdowns ?? calc?.breakdowns ?? {};
  useEffect(() => {
    if (!calc || session.busy || focus) return;
    const stats = selectedMinion ? Object.entries(selectedMinion.stats).map(([id, value]) => ({ id, value })) : calc.stats;
    if (open === null || !stats.some(stat => stat.id === open)) setOpen(defaultMetric(stats, !!(selectedMinion?.skill_id ?? calc.main_skill)));
  }, [calc, selectedMinion, session.busy, focus, open]);
  useEffect(() => {
    if (calc && actor !== 'player' && !selectedMinion) {
      setActor('player');
      setOpen(null);
    }
  }, [calc, actor, selectedMinion]);
  // Later results keep following the calculated skill's minion until the player picks an actor or
  // arrives from a specific stat. Declared before the focus effect, which wins on mount.
  const actorChosen = useRef(false);
  const mainMinionKey = followedActorKey(calc);
  useEffect(() => {
    if (actorChosen.current || mainMinionKey === null || actor === mainMinionKey) return;
    setActor(mainMinionKey);
    setOpen(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- follows the calculated skill's minion only.
  }, [mainMinionKey]);
  useEffect(() => {
    if (!focusMinionDetail.current || !selectedMinion || !open) return;
    focusMinionDetail.current = false;
    detailHeading.current?.focus({ preventScroll: true });
  }, [selectedMinion, open]);
  const cache = useRef(new StatDetailCache());
  const relatedCache = useRef(new WeakMap<CalculateBuildResponse, Record<string, Record<string, Breakdown>>>());
  const [loaded, setLoaded] = useState<{ calc: typeof calc; id: string; actor: string; detail: Breakdown | null; related?: Record<string, Breakdown>; error?: string } | null>(null);
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    if (!calc || session.busy || view !== 'metrics' || !open || detailKind(selectedMinion ?? calc, open) === 'none' || (actorBreakdowns[open] && resistanceIds(open).every(id => actorBreakdowns[id]))) return;
    const request = session.currentRequest();
    if (!request) return;
    let cancelled = false;
    cache.current.load(calc, `${actor}:${open}`, async () => {
      const result = await (await getBackend()).calculateBuild({ ...request, breakdowns: [open, ...resistanceIds(open)], breakdown_actor: selectedMinion ? { kind: 'minion', index: selectedMinion.index } : { kind: 'player' } });
      const related = (selectedMinion ? result.minions?.find(minion => minion.index === selectedMinion.index)?.breakdowns : result.breakdowns) ?? {};
      const actors = relatedCache.current.get(calc) ?? {};
      actors[actor] = { ...actors[actor], ...related };
      relatedCache.current.set(calc, actors);
      return related[open] ?? null;
    }).then(detail => {
      if (!cancelled) setLoaded({ calc, id: open, actor, detail, related: relatedCache.current.get(calc)?.[actor] });
    }).catch(error => {
      if (!cancelled) setLoaded({ calc, id: open, actor, detail: null, error: formatApiError(error) });
    });
    return () => { cancelled = true; };
  }, [calc, open, actor, selectedMinion, actorBreakdowns, session.currentRequest, session.busy, retry, view]);

  // 侧边栏跳转：展开目标 breakdown 并滚动到可视区（渲染提交后再滚）。
  // 消费后立即让父级清空 focus——否则切走再切回本页签会带着旧 focus 重新
  // 挂载，scrollIntoView 再次触发，页面整体错位。
  useEffect(() => {
    if (!focus) return;
    const overview = focus.id === 'skill-metrics';
    const minionFocus = /^Minion:(\d+):(.+)$/.exec(focus.id);
    const stat = minionFocus?.[2] ?? focus.id;
    setView(overview ? 'skills' : 'metrics');
    setCategory('all');
    setCommonOnly(false);
    setQuery('');
    if (!overview) {
      const minion = minionFocus && calc?.minions?.find(row => row.index === Number(minionFocus[1]));
      actorChosen.current = true;
      setActor(minion ? actorKey(minion) : 'player');
      setOpen(stat);
    }
    const frame = requestAnimationFrame(() => {
      const target = document.getElementById(overview ? 'skill-metrics-heading' : 'calcs-detail-heading');
      target?.focus({ preventScroll: true });
      target?.scrollIntoView({ block: 'start', behavior: 'smooth' });
      onFocusConsumed?.();
    });
    return () => cancelAnimationFrame(frame);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focus]);

  const [query, setQuery] = useState('');

  // 分节：按 display_catalog 分类分组全部聚合量（不在目录里的进「其他聚合量」），
  // 组内保持 stats 的目录顺序，目录外条目按名称排序垫底。
  const sections = useMemo(() => {
    if (!calc) return [];
    const catOf = new Map<string, DisplayStatCategory>(calc.stats.map((s) => [s.id, s.category]));
    const catalogOrder = new Map<string, number>(calc.stats.map((s, i) => [s.id, i]));
    const q = query.trim().toLowerCase();
    const names = selectedMinion ? Object.keys(selectedMinion.stats) : calc.stats.map(stat => stat.id);
    const filteredNames = names
      .filter(
        (name) =>
          (q === '' ||
          name.toLowerCase().includes(q) ||
          statNameLabel(lang, name).toLowerCase().includes(q)),
      )
      .sort((a, b) => {
        const ia = catalogOrder.get(a) ?? Number.MAX_SAFE_INTEGER;
        const ib = catalogOrder.get(b) ?? Number.MAX_SAFE_INTEGER;
        return ia !== ib ? ia - ib : a.localeCompare(b);
      });
    const byCat = new Map<string, string[]>();
    for (const name of filteredNames) {
      const cat = catOf.get(name) ?? 'Other';
      if (!byCat.has(cat)) byCat.set(cat, []);
      byCat.get(cat)!.push(name);
    }
    const CAT_ORDER: string[] = [
      'Offence', 'HitDamage', 'DotDamage', 'Ailment', 'SkillMechanics',
      'Defence', 'Resistance', 'Avoidance', 'Mitigation',
      'Resource', 'Recovery', 'Degen', 'Cost', 'Requirement', 'Minion', 'Utility', 'Other',
    ];
    return CAT_ORDER.filter((c) => byCat.has(c)).map((category) => ({
      category,
      names: byCat.get(category)!,
    }));
  }, [calc, query, lang, selectedMinion]);

  const values = selectedMinion ? new Map(Object.entries(selectedMinion.stats)) : statMap(calc?.stats ?? []);
  // A missing poison also has a zero cap; only a calculated poison uses it as no limit.
  const unboundedPoison = values.get('PoisonStacksMax') === 0 && (values.get('PoisonChance') ?? 0) > 0;
  const metricValue = (name: string) => name === 'PoisonStacksMax' && unboundedPoison
    ? '∞' : formatStat(name, values.get(name) ?? null, lang);
  const actorName = selectedMinion ? `${tt('minion.title')} · ${minionName(selectedMinion) || tt('minion.unavailable')} #${selectedMinion.index + 1}` : tt('minion.player');
  const actorLabel = tt('calcs.actorEyebrow').replace('{actor}', actorName);
  const actorPicker = (calc?.minions?.length ?? 0) > 0 && <label className="calcs-actor-picker">
    <span>{tt('minion.actor')}</span>
    <AppSelectControl aria-label={tt('minion.actor')} value={selectedMinion ? actorKey(selectedMinion) : 'player'}
      onChange={event => { actorChosen.current = true; setActor(event.target.value); setOpen(null); }}>
      <option value="player">{tt('minion.player')}</option>
      {(calc?.minions ?? []).map(minion => <option key={actorKey(minion)} value={actorKey(minion)}>
        {tt('minion.title')}{colon(lang)}{minionName(minion) || tt('minion.unavailable')} · {minion.index + 1}
      </option>)}
    </AppSelectControl>
  </label>;
  const detail = (name: string) => {
    const kind = detailKind(selectedMinion ?? calc!, name);
    const current = loaded?.calc === calc && loaded.actor === actor && loaded.id === name ? loaded : null;
    const breakdown = actorBreakdowns[name] ?? current?.detail;
    const emptyKey = detailEmptyKey(kind, values.get(name), breakdown);
    if (session.busy && !breakdown) return <p role="status" className="breakdown-detail">{calcsLabel(lang, 'stale')}</p>;
    return kind === 'none' ? <p className="breakdown-detail" role="status">{tt('calcs.noBreakdown')}</p>
      : breakdown ? emptyKey ? <p className="breakdown-detail" role="status">{tt(emptyKey)}</p>
        : <BreakdownTable breakdown={breakdown} lang={lang} stat={name} unboundedStacks={unboundedPoison && name.startsWith('Poison')} values={values} related={{ ...actorBreakdowns, ...current?.related }} onSelect={id => { setOpen(id); detailHeading.current?.focus({ preventScroll: true }); }} />
      : current?.error ? <div className="breakdown-detail" role="alert">{current.error} <AppButton onClick={() => { setLoaded(null); setRetry(value => value + 1); }}>{tt('calcs.retry')}</AppButton></div>
      : <p className="breakdown-detail" role="status">{tt(current ? emptyKey! : 'calcs.loadingDetail')}</p>;
  };

  const calculatedMinion = mainMinion(calc);
  const playerDps = calc?.stats.filter(stat => stat.id === 'CombinedDPS' || stat.id === 'TotalDPS') ?? [];
  const noPlayerDamage = playerDps.length > 0 && playerDps.every(stat => stat.value === 0);
  const minionDamage = !selectedMinion && calculatedMinion && noPlayerDamage && open && values.get(open) === 0
    && (open.endsWith('DPS') || open === 'AverageDamage');
  const scope = selectedMinion ?? calc;
  const skillId = selectedMinion?.skill_id ?? calc?.main_skill?.skill_id;
  const tabs = [
    ['metrics', calcsLabel(lang, 'metrics')],
    ['skills', tt('calcs.skillMetrics')],
    ['full-dps', calcsLabel(lang, 'fullDps')],
    ['attribution', tt('calcs.attribution')],
    ['ledger', words(lang, 'Modifier ledger', '词缀账本', '詞綴帳本')],
  ] as const;
  const visibleSections = sections.filter(section => category === 'all' || category === section.category)
    .map(section => ({ ...section, names: commonOnly && !query.trim() && category === 'all'
      ? section.names.filter(name => COMMON_METRICS.some(id => id === name) || name === open) : section.names }))
    .filter(section => section.names.length > 0);
  return <section className="ui-page calcs-page calcs-workspace" aria-labelledby="calcs-heading" aria-busy={session.busy}>
    <PageHeader id="calcs-heading" title={tt('calcs.title')} description={calcsLabel(lang, 'detailHint')} />
    <Tabs className="flex-col" value={view} onValueChange={setView}>
      <TabsList variant="line" className="calcs-view-tabs group-data-horizontal/tabs:h-auto" aria-label={tt('calcs.title')}>
        {tabs.map(([value, label]) => <TabsTrigger key={value} className="calcs-view-tab" value={value}>{label}</TabsTrigger>)}
      </TabsList>
      <TabsContent className="min-w-0 w-full" value="metrics">
        {!calc ? <p className="calcs-hint" role={session.calcError ? 'alert' : 'status'}>{session.calcError || calcsLabel(lang, 'noCalc')}</p> : <div className="calcs-explorer">
          <div className="calcs-metric-browser" id="calcs-metric-browser" tabIndex={-1}>
            <div className="calcs-context-bar" aria-label={calcsLabel(lang, 'context')}>
              {actorPicker || <strong>{actorLabel}</strong>}
              {skillId && <span>{tt('calcs.skill')}{colon(lang)}<strong>{skillName(skillId)}</strong></span>}
              {session.busy && <span role="status">{calcsLabel(lang, 'stale')}</span>}
            </div>
            <div className="calcs-filters">
              <div className="calcs-metric-scope segmented" role="group" aria-label={words(lang, 'Metric list', '指标范围', '指標範圍')}>
                <AppButton aria-pressed={commonOnly && !query.trim() && category === 'all'} onClick={() => { setCommonOnly(true); setQuery(''); setCategory('all'); }}>{words(lang, 'Common', '常用指标', '常用指標')}</AppButton>
                <AppButton aria-pressed={!commonOnly || !!query.trim() || category !== 'all'} onClick={() => setCommonOnly(false)}>{words(lang, 'All metrics', '全部指标', '全部指標')}</AppButton>
              </div>
              <AppInput className="calcs-search" type="search" placeholder={tt('calcs.search')} aria-label={tt('calcs.search')}
                value={query} onChange={event => setQuery(event.target.value)} />
              <label className="calcs-category-picker"><span className="sr-only">{calcsLabel(lang, 'categories')}</span>
                <AppSelectControl className="w-full" aria-label={calcsLabel(lang, 'categories')} value={category} onChange={event => setCategory(event.target.value)}>
                  <option value="all">{calcsLabel(lang, 'all')}</option>
                  {category !== 'all' && !sections.some(section => section.category === category) && <option value={category}>{statCategoryLabel(lang, category)}</option>}
                  {sections.map(section => <option key={section.category} value={section.category}>{statCategoryLabel(lang, section.category)} ({section.names.length})</option>)}
                </AppSelectControl>
              </label>
            </div>
            {visibleSections.length === 0 && <div className="search-empty" role="status">
              <p>{tt('common.noResults')}</p>
              <AppButton onClick={() => { setQuery(''); setCategory('all'); }}>{tt('common.clearSearch')}</AppButton>
            </div>}
            <div className="calcs-metric-list">
              {visibleSections.map(section => <section key={section.category} className="calcs-section">
                <h3 className="calcs-section-title">{statCategoryLabel(lang, section.category)}</h3>
                {section.names.map(name => <AppButton key={name} id={`breakdown-${name}`} className="calcs-metric-row h-auto min-h-11 w-full justify-between whitespace-normal border-transparent bg-transparent text-left aria-pressed:border-ring aria-pressed:bg-accent aria-pressed:text-accent-foreground"
                  aria-pressed={open === name} aria-controls="calcs-detail" data-detail-kind={detailKind(scope!, name)}
                  onClick={() => { setOpen(name); requestAnimationFrame(() => {
                    const heading = document.getElementById('calcs-detail-heading');
                    heading?.focus({ preventScroll: true });
                    if (window.matchMedia('(max-width: 760px)').matches) heading?.scrollIntoView({ block: 'start' });
                  }); }}>
                  <span>{statNameLabel(lang, name)}</span>
                  <strong>{metricValue(name)}</strong>
                </AppButton>)}
              </section>)}
            </div>
          </div>
          <section id="calcs-detail" className="calcs-detail-pane" data-empty={!open || undefined} aria-labelledby="calcs-detail-heading">
            <div className="calcs-detail-header">
              <AppButton className="calcs-back-to-list hidden max-[760px]:inline-flex" onClick={() => { const browser = document.getElementById('calcs-metric-browser'); browser?.focus(); browser?.scrollIntoView({ block: 'start' }); }}>{calcsLabel(lang, 'back')}</AppButton>
              {open && <p className="calcs-eyebrow">{actorLabel}</p>}
              <h3 id="calcs-detail-heading" ref={detailHeading} tabIndex={-1}>{open ? statNameLabel(lang, open) : calcsLabel(lang, 'choose')}</h3>
              {open && <div className="calcs-detail-value"><strong>{metricValue(open)}</strong></div>}
              {open && metricMeaning(open, lang) && <p className="metric-meaning">{metricMeaning(open, lang)}</p>}
            </div>
            {minionDamage && <div className="calcs-minion-damage">
              <p className="breakdown-note">{tt('calcs.minionDamage').replace('{skill}', skillName(calc!.main_skill?.skill_id ?? calculatedMinion.summoned_by))}</p>
              <AppButton onClick={() => {
                focusMinionDetail.current = true;
                actorChosen.current = true;
                setActor(actorKey(calculatedMinion));
                setOpen(defaultMetric(Object.entries(calculatedMinion.stats).map(([id, value]) => ({ id, value })), !!(calculatedMinion.skill_id ?? calc?.main_skill)));
              }}>{tt('calcs.viewMinion').replace('{minion}', minionName(calculatedMinion) || tt('minion.unavailable'))}</AppButton>
            </div>}
            {open && detail(open)}
          </section>
        </div>}
      </TabsContent>
      <TabsContent className="min-w-0 w-full" value="skills"><SkillMetricsView session={session} lang={lang} /></TabsContent>
      <TabsContent className="min-w-0 w-full" value="full-dps">
        {session.busy && <p role="status" className="calcs-hint">{calcsLabel(lang, 'stale')}</p>}
        <FullDpsView calc={calc} lang={lang} />
      </TabsContent>
      <TabsContent className="min-w-0 w-full" value="attribution"><AttributionView session={session} lang={lang} /></TabsContent>
      <TabsContent className="min-w-0 w-full" value="ledger"><LedgerView session={session} lang={lang} /></TabsContent>
    </Tabs>
  </section>;
}
