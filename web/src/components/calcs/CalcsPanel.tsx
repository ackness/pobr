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
} from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { STAT_SECTIONS, formatStatValue, statMap } from '../../lib/statDisplay';
import {
  bindT,
  damageTypeLabel,
  grantedSourceLabel,
  originKindLabel,
  originLabel,
  modTypeLabel,
  slotLabel,
  statCategoryLabel,
  statNameLabel,
  type Lang,
} from '../../lib/i18n';
import { getBackend } from '../../api/backend';
import { useEffect } from 'react';
import { useSkillName, useSkillCatalog } from '../../hooks/useSkillName';
import './calcs.css';
import { detailKind, detailFormula, StatDetailCache } from '../../lib/statDetails';
import { AppSelectControl } from '../shared/controls';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '../ui/tabs';
import { calcsLabel } from './calcsLabels';
import { SkillFormControls } from '../skills/SkillFormControls';
import { highestDpsSkill } from '../../lib/skillSelection';
import { LedgerView } from '../../app/pages/calcs/LedgerView';
import { words } from '../../app/text';

interface Props {
  session: BuildSession;
  lang: Lang;
  /** 侧边栏点击跳转：要展开的 breakdown（对象引用每次点击新建，作为 effect 触发键）。 */
  focus?: { id: string } | null;
  /** 跳转消费完毕的回调（父级清空 focus，防止页签切回时重复触发滚动）。 */
  onFocusConsumed?: () => void;
}

const MOD_TYPE_ORDER = ['BASE', 'INC', 'MORE', 'OVERRIDE', 'FLAG', 'LIST'];

/** 数字统一格式：千分位 + 最多 2 位小数（清掉浮点尾巴）。 */
function fmtNum(value: number): string {
  return value.toLocaleString('en-US', { maximumFractionDigits: 2 });
}

/** 词条展示清洗：剥 `[A|B]` 内部标注。 */
function cleanModText(text: string): string {
  return text
    .replace(/\[([^\]|]*)\|([^\]]*)\]/g, '$2')
    .replace(/\[([^\]]*)\]/g, '$1');
}

/** How a formula step changes the running value, e.g. `+120`, `×1.35 (+35%)`, `≤ 75`. */
function formulaOperand(step: FormulaStep): string {
  const n = fmtNum(step.operand);
  const signed = step.operand < 0 ? `−${fmtNum(-step.operand)}` : `+${n}`;
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
      return `×${fmtNum(1 + step.operand / 100)} (${signed}%)`;
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

/** The per-type table behind the average hit: base range, scaling, and both averages. */
function HitDamageView({ damage, lang }: { damage: HitDamageTable; lang: Lang }) {
  const tt = bindT(lang);
  const range = (min: number, max: number) => `${fmtNum(min)} – ${fmtNum(max)}`;
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
              <td className="mod-value">{fmtNum(row.increased)}%</td>
              <td className="mod-value">
                ×{fmtNum(row.more)}
                {row.more_min !== 1 && ` · ${tt('calcs.damage.minOnly')} ×${fmtNum(row.more_min)}`}
                {row.more_max !== 1 && ` · ${tt('calcs.damage.maxOnly')} ×${fmtNum(row.more_max)}`}
              </td>
              <td className="mod-value">{range(row.min, row.max)}</td>
              <td className="mod-value">{fmtNum(row.avg)}</td>
              <td className="mod-value">{fmtNum(row.crit_avg)}</td>
            </tr>
          ))}
        </tbody>
        <tfoot>
          <tr>
            <th scope="row" colSpan={5}>{tt('calcs.damage.total')}</th>
            <td className="mod-value">{fmtNum(sum('avg'))}</td>
            <td className="mod-value">{fmtNum(sum('crit_avg'))}</td>
          </tr>
        </tfoot>
      </table>
      {damage.damage_effect !== 1 && (
        <p className="breakdown-note">{tt('calcs.damage.effect')}: ×{fmtNum(damage.damage_effect)}</p>
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
                  <td className="mod-value">×{fmtNum(source.added_effect)}</td>
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
          <summary>
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
              <td className="mod-value">{fmtNum(row.average)}</td>
              <td className="mod-value">{row.resist === null ? tt('calcs.mitigation.armour') : `${fmtNum(row.resist)}%`}</td>
              <td className="mod-value">{fmtNum(row.reduction)}%</td>
              <td className="mod-value">×{fmtNum(row.taken)}</td>
              <td className="mod-value">×{fmtNum(row.multiplier)}</td>
              <td className="mod-value">{fmtNum(row.mitigated)}</td>
            </tr>
          ))}
        </tbody>
        <tfoot>
          <tr>
            <th scope="row">{tt('calcs.damage.total')}</th>
            <td className="mod-value">{fmtNum(sum('average'))}</td>
            <td colSpan={4} />
            <td className="mod-value">{fmtNum(sum('mitigated'))}</td>
          </tr>
        </tfoot>
      </table>
      {critsDiffer && <p className="breakdown-note">{tt('calcs.mitigation.critNote')}</p>}
      {rows
        .filter((row) => row.resist_inverted !== 0)
        .map((row) => (
          <p key={row.damage_type} className="breakdown-note">
            {tt('calcs.mitigation.invertedNote')
              .replace('{chance}', fmtNum(row.resist_inverted))
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
        <details key={group.key} className={`damage-lines-group enemy-lines-${group.key.toLowerCase()}`}>
          <summary>
            {damageTypeLabel(lang, group.type)} · {group.label}
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
                <td className="mod-value">{fmtNum(row.base)}</td>
                <td className="mod-value">{fmtNum(row.increased)}%</td>
                <td className="mod-value">×{fmtNum(row.more)}</td>
                <td className="mod-value">{fmtNum(row.dot_multiplier)}%</td>
                {showAura && <td className="mod-value">×{fmtNum(row.aura)}</td>}
                <td className="mod-value">×{fmtNum(row.enemy)}</td>
                <td className="mod-value">{fmtNum(row.total)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="damage-lines">
        {rows.filter((row) => row.mods.length > 0).map((row) => (
          <details key={row.damage_type} className={`damage-lines-group damage-lines-${row.damage_type.toLowerCase()}`}>
            <summary>
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
  const seconds = (value: number) => `${fmtNum(value)}${tt('calcs.ailment.seconds')}`;
  const rows: [Key, string][] = [
    ['sourceHit', fmtNum(ailment.source_hit)],
    ['sourceCrit', fmtNum(ailment.source_crit)],
    ['critShare', `${fmtNum(ailment.crit_share)}%`],
    ['hitChance', `${fmtNum(ailment.hit_chance)}%`],
    ['applyChance', `${fmtNum(ailment.apply_chance)}%`],
    ...(ailment.base_duration !== ailment.duration ? [['baseDuration', seconds(ailment.base_duration)] as [Key, string]] : []),
    ...(ailment.enemy_duration !== 1 ? [['enemyDuration', `×${fmtNum(ailment.enemy_duration)}`] as [Key, string]] : []),
    ...(ailment.rate !== 1 ? [['rate', `÷${fmtNum(ailment.rate)}`] as [Key, string]] : []),
    ['duration', seconds(ailment.duration)],
    ['hitsPerSecond', fmtNum(ailment.hits_per_second)],
    ...(ailment.stacks > 0 ? [['stacks', fmtNum(ailment.stacks)] as [Key, string]] : []),
    ['maxStacks', fmtNum(ailment.max_stacks)],
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
                  <td className="mod-value">{fmtNum(row.hit)}</td>
                  <td className="mod-value">{fmtNum(row.crit)}</td>
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
            {tt('calcs.ailment.rollNote').replace('{roll}', fmtNum(ailment.damage_roll))}
          </p>
        )}
        {ailment.stacks === 0 && <p className="breakdown-note">{tt('calcs.ailment.noRateNote')}</p>}
      </div>
      {lineGroups.length > 0 && (
        <div className="damage-lines ailment-lines">
          {lineGroups.map(([key, mods]) => (
            <details key={key} className={`damage-lines-group ailment-lines-${key}`}>
              <summary>
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
              <td className="mod-value">{fmtNum(row.base)}</td>
              <td className="mod-value">{fmtNum(row.increased)}%</td>
              <td className="mod-value">×{fmtNum(row.more)}</td>
              <td className="mod-value">{fmtNum(row.extra)}</td>
            </tr>
          ))}
        </tbody>
        <tfoot>
          <tr>
            <th scope="row" colSpan={4}>{tt('calcs.damage.total')}</th>
            <td className="mod-value">{fmtNum(slots.reduce((total, row) => total + row.extra, 0))}</td>
          </tr>
        </tfoot>
      </table>
    </div>
  );
}

function ReloadView({ reload, lang }: { reload: ReloadDetail; lang: Lang }) {
  const tt = bindT(lang);
  const seconds = (value: number) => `${fmtNum(value)}${tt('calcs.ailment.seconds')}`;
  type Key = 'firingRate' | 'bolts' | 'firingTime' | 'reloadTime' | 'rate';
  const rows: [Key, string][] = [
    ['firingRate', fmtNum(reload.firing_rate)],
    ['bolts', reload.bolts === null ? '∞' : fmtNum(reload.bolts)],
    ['firingTime', seconds(reload.firing_time)],
    ['reloadTime', seconds(reload.reload_time)],
    ['rate', fmtNum(reload.rate)],
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
function ModLines({ mods, lang }: { mods: BreakdownMod[]; lang: Lang }) {
  const tt = bindT(lang);
  // 中文界面：词条行批量反查翻译（清洗标注后送翻译器；结果缓存本组件）。
  const [zhLines, setZhLines] = useState<Record<string, string>>({});
  useEffect(() => {
    setZhLines({});
    if (lang === 'en-US') return;
    const pending = [
      ...new Set(
        mods
          .map((m) => m.source_text)
          .filter((t): t is string => !!t)
          .map(cleanModText),
      ),
    ];
    if (pending.length === 0) return;
    let cancelled = false;
    getBackend()
      .then((b) => b.translateLines(pending, lang))
      .then((translated) => {
        if (cancelled) return;
        const map: Record<string, string> = {};
        pending.forEach((en, i) => {
          if (translated[i] !== en) map[en] = translated[i];
        });
        setZhLines(map);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [mods, lang]);

  const modText = (raw: string | null) => {
    if (!raw) return tt('calcs.baseDerived');
    const cleaned = cleanModText(raw);
    return lang !== 'en-US' ? (zhLines[cleaned] ?? cleaned) : cleaned;
  };
  return (
    <div className="breakdown-scroll">
      <table className="breakdown-table">
        <thead>
          <tr>
            <th>{tt('calcs.type')}</th>
            <th>{tt('calcs.value')}</th>
            <th>{tt('calcs.modifier')}</th>
            <th>{tt('calcs.source')}</th>
          </tr>
        </thead>
        <tbody>
          {[...mods]
            .sort(
              (a, b) =>
                MOD_TYPE_ORDER.indexOf(a.mod_type) - MOD_TYPE_ORDER.indexOf(b.mod_type) ||
                Number(b.active) - Number(a.active),
            )
            .map((mod, i) => (
              <tr key={i} className={mod.active ? undefined : 'is-inactive'}>
                <td className={`mod-type mod-type-${mod.mod_type.toLowerCase()}`}>{modTypeLabel(lang, mod.mod_type)}</td>
                <td className="mod-value">{mod.value === null ? '—' : fmtNum(mod.value)}</td>
                <td className="mod-text">
                  {modText(mod.source_text)}
                  {!mod.active && <span className="mod-inactive-tag">{tt('calcs.inactive')}</span>}
                </td>
                <td className="mod-origin">
                  {mod.origin_kind
                    ? `${originKindLabel(lang, mod.origin_kind)}${
                        mod.slot ? ` · ${slotLabel(lang, mod.slot)}` : ''
                      }`
                    : '—'}
                </td>
              </tr>
            ))}
        </tbody>
      </table>
    </div>
  );
}

export function BreakdownTable({ breakdown, lang }: { breakdown: Breakdown; lang: Lang }) {
  const tt = bindT(lang);
  if (breakdown.derived) {
    const derived = breakdown.derived;
    const sourceHints = derived.context.filter(input => input.id.startsWith('EnergyNeeds'));
    const context = derived.context.filter(input => !input.id.startsWith('EnergyNeeds'));
    return <div className="breakdown-detail breakdown-derived">
      {sourceHints.map(hint => <p className="breakdown-note" key={hint.id}>{statNameLabel(lang, hint.id)}</p>)}
      <p className="derived-formula">{detailFormula(derived.formula, derived.inputs, lang)}</p>
      <table className="breakdown-table"><caption>{tt('calcs.inputs')}</caption><tbody>
        {derived.inputs.map(input => <tr key={input.id}><th scope="row">{statNameLabel(lang, input.id)}</th><td>{formatStatValue(input.value, 'float2')}</td></tr>)}
      </tbody></table>
      {context.length > 0 && <table className="breakdown-table"><caption>{tt('calcs.context')}</caption><tbody>
        {context.map(input => <tr key={input.id}><th scope="row">{statNameLabel(lang, input.id)}</th><td>{formatStatValue(input.value, 'float2')}</td></tr>)}
      </tbody></table>}
    </div>;
  }
  if (breakdown.kind === 'sources' && breakdown.actor?.kind !== 'minion') {
    const groups = breakdown.sources?.filter(group => group.mods.length > 0) ?? [];
    return <div className="breakdown-detail breakdown-sources">
      <p className="breakdown-note">{tt('calcs.sourcesNote')}</p>
      {groups.length === 0 && <p>{tt('calcs.noSources')}</p>}
      {groups.map(group => <section key={group.name}>
        <h4>{statNameLabel(lang, group.name)}</h4>
        {group.mods.some(mod => mod.active && mod.mod_type === 'MORE') && <p className="breakdown-note">{tt(group.more_precision === null ? 'calcs.moreRounded' : 'calcs.moreFloored')} {group.more_precision ?? 2}</p>}
        <BreakdownTable breakdown={{ ...group, formula: null }} lang={lang} />
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
      {breakdown.damage && <HitDamageView damage={breakdown.damage} lang={lang} />}
      {breakdown.skill_dot && <SkillDotView rows={breakdown.skill_dot} lang={lang} />}
      {breakdown.formula ? (
        <table className="breakdown-formula" aria-label={tt('calcs.formula')}>
          <caption>{tt('calcs.formula')}</caption>
          <tbody>
            {breakdown.formula.steps.map((step, i) => (
              <tr key={i} className={`formula-step formula-${step.op}`}>
                <th scope="row">{tt(`calcs.step.${step.op}`)}</th>
                <td className="formula-operand">{formulaOperand(step)}</td>
                <td className="formula-result">{fmtNum(step.result)}</td>
              </tr>
            ))}
            <tr className="formula-final">
              <th scope="row">{tt('calcs.formulaResult')}</th>
              <td />
              <td className="formula-result">{fmtNum(breakdown.formula.value)}</td>
            </tr>
          </tbody>
        </table>
      ) : (
        <>
          <div className="breakdown-summary">
            <span>
              {tt('calcs.baseTotal')}: <strong>{fmtNum(breakdown.base_total)}</strong>
            </span>
            <span>
              {tt('calcs.incTotal')}: <strong>{fmtNum(breakdown.inc_total)}%</strong>
            </span>
            {breakdown.more_total !== 1 && (
              <span>
                {tt('calcs.moreTotal')}: <strong>×{fmtNum(breakdown.more_total)}</strong>
              </span>
            )}
          </div>
          <p className="breakdown-note breakdown-totals-note">{tt('calcs.totalsNote')}</p>
        </>
      )}
      {breakdown.mitigation && breakdown.mitigation.length > 0 && (
        <>
          <MitigationView rows={breakdown.mitigation} lang={lang} />
          <EnemyLinesView rows={breakdown.mitigation} lang={lang} />
        </>
      )}
      {breakdown.ailment && <AilmentView ailment={breakdown.ailment} lang={lang} />}
      {breakdown.reload && <ReloadView reload={breakdown.reload} lang={lang} />}
      {breakdown.slots && breakdown.slots.length > 0 && <DefenceSlotsView slots={breakdown.slots} lang={lang} />}
      {hands.length > 0 && <p className="breakdown-note breakdown-hands-note">{tt('calcs.hand.note')}</p>}
      {hands.map((hand) => (
        <section key={hand.hand} className={`breakdown-hand breakdown-hand-${hand.hand}`} aria-label={tt(`calcs.hand.${hand.hand}`)}>
          <h4 className="breakdown-hand-title">{tt(`calcs.hand.${hand.hand}`)}</h4>
          <BreakdownTable breakdown={hand} lang={lang} />
        </section>
      ))}
      {!linesPerHand && [...breakdown.mods, ...damageLines].some((m) => !m.active) && (
        <p className="breakdown-note">{tt('calcs.inactiveNote')}</p>
      )}
      {breakdown.damage && <DamageLinesView damage={breakdown.damage} lang={lang} />}
      {!linesPerHand && breakdown.mods.length > 0 && <ModLines mods={breakdown.mods} lang={lang} />}
    </div>
  );
}

function FullDpsView({ session, lang }: { session: BuildSession; lang: Lang }) {
  const tt = bindT(lang);
  const skillName = useSkillName(lang);
  const full = session.calc?.full_dps;
  return <section className="full-dps-view" aria-labelledby="full-dps-heading">
    <h3 id="full-dps-heading">{calcsLabel(lang, 'fullDps')}</h3>
    <p className="calcs-hint">{tt('skills.fullDpsHint')}</p>
    {full ? <>
      <div className="breakdown-summary">
        <span>{statNameLabel(lang, 'FullDPS')}: <strong>{fmtNum(full.dps)}</strong></span>
        <span>{statNameLabel(lang, 'FullDotDPS')}: <strong>{fmtNum(full.dot_dps)}</strong></span>
      </div>
      <div className="breakdown-scroll"><table className="breakdown-table full-dps-table">
        <caption>{tt('calcs.fullDpsContribution')}</caption>
        <thead><tr><th>{tt('calcs.skill')}</th><th>{tt('skills.count')}</th><th>{tt('calcs.fullDpsContribution')}</th><th>{tt('calcs.diagnostics')}</th></tr></thead>
        <tbody>{full.skills.map(row => <tr key={`${row.group_index}-${row.active_skill_index}`}>
          <th>{tt('calcs.group')} {row.group_index + 1} · {skillName(row.skill_id)}</th><td>{fmtNum(row.count)}</td>
          <td>{row.error ? '—' : fmtNum(row.dps)}</td><td>{row.error}{row.unsupported_modifiers.length > 0 && <details><summary>{tt('calcs.unsupported')} ({row.unsupported_modifiers.length})</summary>
            <ul>{row.unsupported_modifiers.map((issue, index) => <li key={index}>{issue}</li>)}</ul></details>}</td>
        </tr>)}</tbody>
      </table></div>
    </> : <p role="status" className="calcs-hint">{calcsLabel(lang, 'fullDpsEmpty')}</p>}
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

  const skillName = useSkillName(lang);
  const catalog = useSkillCatalog();
  const mainIndex = session.calcParams.main_socket_group;
  const highest = report ? highestDpsSkill(report.skills, session.socketGroups) : null;
  const columns = [
    ['combined_dps', 'sidebar.combinedDps'],
    ['average_hit', 'calcs.averageHit'],
    ['action_rate', 'calcs.actionRate'],
    ['mana_cost', 'calcs.manaCost'],
    ['life_cost', 'calcs.lifeCost'],
    ['cooldown', 'calcs.cooldown'],
  ] as const;

  return (
    <section className="ui-card attribution-view" aria-labelledby="skill-metrics-heading" aria-busy={running}>
      <h3 id="skill-metrics-heading" tabIndex={-1}>{tt('calcs.skillMetrics')}</h3>
      <p className="calcs-hint">{tt('calcs.skillMetricsHint')}</p>
      {mainIndex !== undefined && <SkillFormControls session={session} lang={lang} groupIndex={mainIndex} catalog={catalog} />}
      {running && <p className="calcs-hint" role="status">{tt('calcs.running')}</p>}
      {error && <div className="calc-error" role="alert">{error}</div>}
      {report?.item_errors.map((issue, index) => (
        <p className="calc-error" key={`${issue.slot}-${index}`}>{slotLabel(lang, issue.slot)}: {issue.message}</p>
      ))}
      {displayed && (displayed.skills.length === 0 ? (
        <p className="calcs-hint">{tt('calcs.skillMetricsEmpty')}</p>
      ) : (
        <div className="breakdown-scroll">
          <table className="breakdown-table skill-metrics-table">
            <thead>
              <tr>
                <th scope="col">{tt('calcs.skill')}</th>
                {columns.map(([key, label]) => <th scope="col" key={key}>{tt(label)}</th>)}
                <th scope="col">{tt('calcs.diagnostics')}</th>
              </tr>
            </thead>
            <tbody>
              {displayed.skills.map(entry => {
                const group = session.socketGroups[entry.group_index];
                const granted = grantedSourceLabel(lang, group?.source);
                const selected = entry.group_index === mainIndex
                  && entry.active_skill_index === (group?.main_active_skill ?? 1);
                const available = !!report && entry.status === 'calculated' && entry.metrics !== null;
                const disabled = !report || session.busy || !group?.enabled;
                const inspect = () => {
                  if (!disabled) session.inspectSkill(entry.group_index, entry.active_skill_index, stateVersion);
                };
                return (
                  <tr key={`${entry.group_index}-${entry.active_skill_index}`}
                    className={`skill-metrics-row${selected ? ' is-main' : ''}${granted ? ' is-granted' : ''}`}
                    onClick={inspect}>
                    <td>
                      <AppButton className="calcs-skill-button h-auto justify-start whitespace-normal text-left" aria-pressed={selected}
                        title={group?.gems.map(gem => skillName(gem.skill_id)).join(' + ')}
                        aria-disabled={disabled}
                        onClick={event => { event.stopPropagation(); inspect(); }}>
                        {tt('calcs.group')} {entry.group_index + 1} · {entry.active_skill_index} · {skillName(entry.skill_id)}
                      </AppButton>
                      {granted && <span className="granted-badge">{granted}</span>}
                      {entry === highest && entry.metrics!.combined_dps > 0 && <span className="granted-badge">{tt('calcs.highestDps')}</span>}
                    </td>
                    {columns.map(([key]) => (
                      <td className="mod-value" key={key}>{available ? formatStatValue(entry.metrics![key], 'float2') : '—'}</td>
                    ))}
                    <td className="skill-metrics-diagnostics">
                      {report && !available && <p className="calc-error">{tt('calcs.unavailable')}: {entry.error || tt('calcs.unavailableHint')}</p>}
                      {available && entry.error && <p className="calc-error">{entry.error}</p>}
                      {report && entry.unsupported_modifiers.length > 0 && (
                        <details onClick={event => event.stopPropagation()}>
                          <summary>{tt('calcs.unsupported')} ({entry.unsupported_modifiers.length})</summary>
                          <ul>{entry.unsupported_modifiers.map((modifier, index) => <li key={index}>{modifier}</li>)}</ul>
                        </details>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      ))}
    </section>
  );
}

function AttributionView({ session, lang }: { session: BuildSession; lang: Lang }) {
  const tt = bindT(lang);
  const [completed, setCompleted] = useState<{ version: number; report: AttributionResponse } | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fields = ['TotalDPS', 'Life', 'EnergyShield', 'TotalEHP'];
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
      if (current === sequence.current) setCompleted({ version, report: result });
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
      return `${tt('calcs.group')} ${Number(id) + 1}${gems ? ` · ${gems}` : ''}`;
    }
    if (kind === 'item' || kind === 'flask') return originLabel(lang, `${kind}.${id}`);
    if (kind === 'limb') return slotLabel(lang, id);
    return id;
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
      {completed && !report && <p className="calcs-hint" role="status">{tt('calcs.attributionStale')}</p>}
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
                  <td key={f}>{formatStatValue(report.baseline[f] ?? null, 'float2')}</td>
                ))}
              </tr>
              {[...report.entries]
                .sort(
                  (a, b) => Math.abs(b.deltas[fields[0]] ?? 0) - Math.abs(a.deltas[fields[0]] ?? 0),
                )
                .map((entry) => (
                  <tr key={`${entry.kind}-${entry.id}`}>
                    <td>
                      <span className="attribution-kind">{entry.kind === 'item' || entry.kind === 'flask'
                        ? originKindLabel(lang, entry.kind === 'item' ? 'Item' : 'Flask')
                        : calcsLabel(lang, entry.kind)}</span> {label(entry.kind, entry.id)}
                    </td>
                    {fields.map((f) => {
                      const delta = entry.deltas[f] ?? 0;
                      const baseline = report.baseline[f] ?? 0;
                      const pct = baseline !== 0 ? (delta / baseline) * 100 : 0;
                      return (
                        <td key={f} className={delta > 0 ? 'delta-pos' : delta < 0 ? 'delta-neg' : ''}>
                          {delta === 0 ? '—' : `${delta > 0 ? '+' : ''}${formatStatValue(delta, 'float2')}`}
                          {Math.abs(pct) >= 0.05 && <span className="delta-pct"> ({pct.toFixed(1)}%)</span>}
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
export function CalcsPanel({ session, lang, focus, onFocusConsumed }: Props) {
  const tt = bindT(lang);
  const skillName = useSkillName(lang);
  const [view, setView] = useState('metrics');
  const [category, setCategory] = useState('all');
  const calc = session.calcError ? null : session.calc;
  const [open, setOpen] = useState<string | null>(null);
  const [actor, setActor] = useState('player');
  const actorKey = (minion: MinionOutput) =>
    `${minion.index}:${minion.minion_id}:${minion.summoned_by}`;
  const selectedMinion = calc?.minions?.find(minion => actorKey(minion) === actor);
  const actorBreakdowns = selectedMinion?.breakdowns ?? calc?.breakdowns ?? {};
  useEffect(() => {
    if (calc && actor !== 'player' && !selectedMinion) {
      setActor('player');
      setOpen(null);
    }
  }, [calc, actor, selectedMinion]);
  const cache = useRef(new StatDetailCache());
  const [loaded, setLoaded] = useState<{ calc: typeof calc; id: string; actor: string; detail: Breakdown | null; error?: string } | null>(null);
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    if (!calc || session.busy || view !== 'metrics' || !open || detailKind(selectedMinion ?? calc, open) === 'none' || actorBreakdowns[open]) return;
    const request = session.currentRequest();
    if (!request) return;
    let cancelled = false;
    cache.current.load(calc, `${actor}:${open}`, async () => {
      const result = await (await getBackend()).calculateBuild({ ...request, breakdowns: [open], breakdown_actor: selectedMinion ? { kind: 'minion', index: selectedMinion.index } : { kind: 'player' } });
      return (selectedMinion ? result.minions?.find(minion => minion.index === selectedMinion.index)?.breakdowns[open] : result.breakdowns[open]) ?? null;
    }).then(detail => {
      if (!cancelled) setLoaded({ calc, id: open, actor, detail });
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
    setQuery('');
    if (!overview) {
      const minion = minionFocus && calc?.minions?.find(row => row.index === Number(minionFocus[1]));
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
          q === '' ||
          name.toLowerCase().includes(q) ||
          statNameLabel(lang, name).toLowerCase().includes(q),
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
  const rowFormat = (name: string) => selectedMinion ? 'float2' as const
    : STAT_SECTIONS.flatMap(section => section.rows).find(row => row.id === name)?.format ?? 'float2';
  const actorLabel = selectedMinion ? `${tt('minion.title')}: ${selectedMinion.name || tt('minion.unavailable')} · ${selectedMinion.index + 1}` : tt('minion.player');
  const actorPicker = (calc?.minions?.length ?? 0) > 0 && <label className="calcs-actor-picker">
    <span>{tt('minion.actor')}</span>
    <AppSelectControl aria-label={tt('minion.actor')} value={selectedMinion ? actorKey(selectedMinion) : 'player'}
      onChange={event => { setActor(event.target.value); setOpen(null); }}>
      <option value="player">{tt('minion.player')}</option>
      {(calc?.minions ?? []).map(minion => <option key={actorKey(minion)} value={actorKey(minion)}>
        {tt('minion.title')}: {minion.name || tt('minion.unavailable')} · {minion.index + 1}
      </option>)}
    </AppSelectControl>
  </label>;
  const detail = (name: string) => {
    const kind = detailKind(selectedMinion ?? calc!, name);
    const current = loaded?.calc === calc && loaded.actor === actor && loaded.id === name ? loaded : null;
    const breakdown = actorBreakdowns[name] ?? current?.detail;
    if (session.busy) return <p role="status" className="breakdown-detail">{calcsLabel(lang, 'stale')}</p>;
    return kind === 'none' ? <p className="breakdown-detail" role="status">{tt('calcs.noBreakdown')}</p>
      : breakdown ? <BreakdownTable breakdown={breakdown} lang={lang} />
      : current?.error ? <div className="breakdown-detail" role="alert">{current.error} <AppButton onClick={() => { setLoaded(null); setRetry(value => value + 1); }}>{tt('calcs.retry')}</AppButton></div>
      : <p className="breakdown-detail" role="status">{tt(current ? 'calcs.noBreakdown' : 'calcs.loadingDetail')}</p>;
  };

  const scope = selectedMinion ?? calc;
  const skillId = selectedMinion?.skill_id ?? calc?.main_skill?.skill_id;
  const tabs = [
    ['metrics', calcsLabel(lang, 'metrics')],
    ['skills', tt('calcs.skillMetrics')],
    ['full-dps', calcsLabel(lang, 'fullDps')],
    ['attribution', tt('calcs.attribution')],
    ['ledger', words(lang, 'Ledger', '词条账本', '詞綴帳本')],
  ] as const;
  const visibleSections = sections.filter(section => category === 'all' || category === section.category);
  return <section className="ui-page calcs-page calcs-workspace" aria-labelledby="calcs-heading">
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
              {skillId && <span>{tt('calcs.skill')}: <strong>{skillName(skillId)}</strong></span>}
              {session.busy && <span role="status">{calcsLabel(lang, 'stale')}</span>}
            </div>
            <div className="calcs-filters">
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
                  <strong>{formatStatValue(values.get(name) ?? null, rowFormat(name), lang)}</strong>
                </AppButton>)}
              </section>)}
            </div>
          </div>
          <section id="calcs-detail" className="calcs-detail-pane" data-empty={!open || undefined} aria-labelledby="calcs-detail-heading">
            <div className="calcs-detail-header">
              <AppButton className="calcs-back-to-list hidden max-[760px]:inline-flex" onClick={() => { const browser = document.getElementById('calcs-metric-browser'); browser?.focus(); browser?.scrollIntoView({ block: 'start' }); }}>{calcsLabel(lang, 'back')}</AppButton>
              {open && <p className="calcs-eyebrow">{actorLabel}</p>}
              <h3 id="calcs-detail-heading" tabIndex={-1}>{open ? statNameLabel(lang, open) : calcsLabel(lang, 'choose')}</h3>
              {open && <div className="calcs-detail-value"><strong>{formatStatValue(values.get(open) ?? null, rowFormat(open), lang)}</strong><span>{calcsLabel(lang, detailKind(scope!, open))}</span></div>}
            </div>
            {open && detail(open)}
          </section>
        </div>}
      </TabsContent>
      <TabsContent className="min-w-0 w-full" value="skills"><SkillMetricsView session={session} lang={lang} /></TabsContent>
      <TabsContent className="min-w-0 w-full" value="full-dps"><FullDpsView session={session} lang={lang} /></TabsContent>
      <TabsContent className="min-w-0 w-full" value="attribution"><AttributionView session={session} lang={lang} /></TabsContent>
      <TabsContent className="min-w-0 w-full" value="ledger"><LedgerView session={session} lang={lang} /></TabsContent>
    </Tabs>
  </section>;
}
