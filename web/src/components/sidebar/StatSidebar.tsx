import type { MainSkillInfo } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useSkillCatalog } from '../../hooks/useSkillName';
import { activeSkillEffects } from '../../lib/skillSelection';
import { skillDisplayName } from '../../lib/skillNames';
import { STAT_SECTIONS, formatStatValue, statMap, type Lang } from '../../lib/statDisplay';
import { bindT, damageTypeLabel } from '../../lib/i18n';
import { AppSelect } from '../shared/AppSelect';
import './sidebar.css';

interface Props {
  session: BuildSession;
  lang: Lang;
  /** 点击有 breakdown 的数值 → 跳 Calcs 页展开明细。 */
  onStatClick?: (id: string) => void;
}

/** 伤害类型 → 语义色 CSS 变量（tokens.css）。 */
const DAMAGE_COLOR: Record<string, string> = {
  Physical: 'color-physical',
  Fire: 'color-fire',
  Cold: 'color-cold',
  Lightning: 'color-lightning',
  Chaos: 'color-chaos',
};

/**
 * Inspected calculation skill and damage breakdown.
 * 数值随每次重算的 calc.main_skill 刷新——装备/天赋一变即时更新。
 */
function MainSkillSection({ session, lang, onStatClick }: Props) {
  const tt = bindT(lang);
  const catalog = useSkillCatalog();
  const skillName = (skillId: string) => skillDisplayName(skillId, lang, catalog);
  const groups = session.socketGroups;
  const mainSkill: MainSkillInfo | null = session.calcError || session.calcParams.main_socket_group === undefined
    ? null : session.calc?.main_skill ?? null;
  // The session fills a missing selection with the highest-DPS skill; until
  // that lands (or when nothing is calculable) no skill is selected.
  const selected = session.calcParams.main_socket_group;

  if (groups.length === 0) return null;

  // A DPS row opens its steps on the Calcs page when the calculation explains it.
  const dpsRow = (id: string, label: string, value: number, color?: string) => {
    const inner = (
      <>
        <dt>{label}</dt>
        <dd style={color ? { color } : undefined}>{formatStatValue(value, 'float2')}</dd>
      </>
    );
    return onStatClick && session.calc?.breakdowns[id] ? (
      <button type="button" className="stat-row stat-row--link" key={id} onClick={() => onStatClick(id)}>
        {inner}
      </button>
    ) : (
      <div className="stat-row" key={id}>{inner}</div>
    );
  };

  const totalAvg = mainSkill?.hit_damage.reduce((sum, part) => sum + part.avg, 0) ?? 0;

  return (
    <section className="stat-section main-skill-section">
      <h3>{tt('sidebar.mainSkill')}</h3>
      <div className="main-skill-select"><AppSelect
        ariaLabel={tt('sidebar.mainSkill')}
        value={selected === undefined ? '' : String(selected)}
        placeholder={tt('sidebar.unselected')}
        disabled={session.busy}
        onChange={(value) => session.updateParams({ main_socket_group: Number(value) })}
        options={groups.map((group, idx) => {
          const effect = catalog.size ? activeSkillEffects(group, catalog)[(group.main_active_skill ?? 1) - 1] : undefined;
          const name = effect ? skillName(effect.skillId) : tt('calcs.unavailable');
          return { value: String(idx), label: `${idx + 1}. ${name}${group.enabled ? '' : ` ${tt('sidebar.disabledGroup')}`}` };
        })}
      /></div>
      {!mainSkill && <p className="main-skill-empty">{tt('sidebar.noMainSkill')}</p>}
      {mainSkill && mainSkill.group_index !== selected && (
        // Keep the resolved calculation identity visible if it differs from the selection.
        <p className="main-skill-empty">
          {tt('sidebar.computedSkill')}: {skillName(mainSkill.skill_id)}
        </p>
      )}
      {mainSkill && (
        <>
          <dl>
            {dpsRow('TotalDPS', tt('sidebar.hitDps'), mainSkill.hit_dps)}
            {mainSkill.dot_dps > 0 && dpsRow('TotalDotDPS', tt('sidebar.dotDps'), mainSkill.dot_dps)}
            {dpsRow('CombinedDPS', tt('sidebar.combinedDps'), mainSkill.combined_dps, 'var(--accent-gold-bright)')}
          </dl>
          {totalAvg > 0 && (
            <div className="damage-share" role="list" aria-label={tt('sidebar.damageShare')}>
              <div className="damage-share-title">{tt('sidebar.damageShare')}</div>
              {[...mainSkill.hit_damage]
                .sort((a, b) => b.avg - a.avg)
                .filter((part) => part.avg > 0)
                .map((part) => {
                  const pct = (part.avg / totalAvg) * 100;
                  const colorVar = DAMAGE_COLOR[part.damage_type];
                  return (
                    <div className="damage-share-row" role="listitem" key={part.damage_type}>
                      <div className="damage-share-head">
                        <span
                          className="damage-share-type"
                          style={colorVar ? { color: `var(--${colorVar})` } : undefined}
                        >
                          {damageTypeLabel(lang, part.damage_type)}
                        </span>
                        <span className="damage-share-value">
                          {formatStatValue(part.avg, 'float2')} · {pct.toFixed(1)}%
                        </span>
                      </div>
                      <div className="damage-share-track">
                        <div
                          className="damage-share-fill"
                          style={{
                            width: `${pct}%`,
                            background: colorVar ? `var(--${colorVar})` : 'var(--accent-gold)',
                          }}
                        />
                      </div>
                    </div>
                  );
                })}
            </div>
          )}
        </>
      )}
    </section>
  );
}

/** PoB2 式左侧常驻 stat 侧边栏：主技能选择/分解 + 分组展示 display_catalog 字段。 */
export function StatSidebar({ session, lang, onStatClick }: Props) {
  const calc = session.calc;
  const values = calc ? statMap(calc.stats) : null;

  return (
    <aside id="character-stats" className="stat-sidebar" aria-label="Character stats">
      <MainSkillSection session={session} lang={lang} onStatClick={onStatClick} />
      {onStatClick && <button className="stat-row stat-row--link" onClick={() => onStatClick('skill-metrics')}>
        {bindT(lang)('calcs.skillMetrics')} →
      </button>}
      {STAT_SECTIONS.map((section) => {
        if (section.requiresSkill && (session.calcParams.main_socket_group === undefined || !calc?.main_skill)) return null;
        const rows = section.rows.filter((row) => {
          if (!values) return !row.hideZero;
          const v = values.get(row.id);
          if (v === undefined) return false;
          return !(row.hideZero && (v === 0 || v === null));
        });
        if (rows.length === 0) return null;
        return (
          <section key={section.title['en-US']} className="stat-section">
            <h3>{section.title[lang]}</h3>
            <dl>
              {rows.map((row) => {
                const clickable = !!onStatClick && !!calc?.breakdowns[row.id];
                const inner = (
                  <>
                    <dt>{row.label[lang]}</dt>
                    <dd style={row.colorVar ? { color: `var(--${row.colorVar})` } : undefined}>
                      {values ? formatStatValue(values.get(row.id) ?? null, row.format, lang) : '—'}
                    </dd>
                  </>
                );
                return clickable ? (
                  <button
                    type="button"
                    className="stat-row stat-row--link"
                    key={row.id}
                    onClick={() => onStatClick(row.id)}
                  >
                    {inner}
                  </button>
                ) : (
                  <div className="stat-row" key={row.id}>
                    {inner}
                  </div>
                );
              })}
            </dl>
          </section>
        );
      })}
    </aside>
  );
}
