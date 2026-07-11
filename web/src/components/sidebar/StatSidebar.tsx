import type { MainSkillInfo } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useSkillName } from '../../hooks/useSkillName';
import { STAT_SECTIONS, formatStatValue, statMap, type Lang } from '../../lib/statDisplay';
import { bindT, damageTypeLabel } from '../../lib/i18n';
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
 * 主技能选择 + 伤害分解（PoB2 左侧栏 Main Skill 下拉的对应物）。
 * 数值随每次重算的 calc.main_skill 刷新——装备/天赋一变即时更新。
 */
function MainSkillSection({ session, lang }: { session: BuildSession; lang: Lang }) {
  const tt = bindT(lang);
  const skillName = useSkillName(lang);
  const groups = session.socketGroups;
  const mainSkill: MainSkillInfo | null = session.calc?.main_skill ?? null;
  // 下拉保持用户显式选择；未选过时跟随引擎实际选中的组。
  const selected = session.calcParams.main_socket_group ?? mainSkill?.group_index ?? 0;

  if (groups.length === 0) return null;

  const totalAvg = mainSkill?.hit_damage.reduce((sum, part) => sum + part.avg, 0) ?? 0;

  return (
    <section className="stat-section main-skill-section">
      <h3>{tt('sidebar.mainSkill')}</h3>
      <select
        className="main-skill-select"
        aria-label={tt('sidebar.mainSkill')}
        value={selected}
        disabled={session.busy}
        onChange={(e) => session.updateParams({ main_socket_group: Number(e.target.value) })}
      >
        {groups.map((group, idx) => {
          const [active] = group.gems;
          const name = active ? skillName(active.skill_id) : tt('skills.emptyGroup');
          return (
            <option key={idx} value={idx}>
              {idx + 1}. {name}
              {group.enabled ? '' : ` ${tt('sidebar.disabledGroup')}`}
            </option>
          );
        })}
      </select>
      {!mainSkill && <p className="main-skill-empty">{tt('sidebar.noMainSkill')}</p>}
      {mainSkill && mainSkill.group_index !== selected && (
        // 选中组无伤害技能时引擎回退到别组（resolve_main_skill 语义）——标明实际计算对象。
        <p className="main-skill-empty">
          {tt('sidebar.computedSkill')}: {skillName(mainSkill.skill_id)}
        </p>
      )}
      {mainSkill && (
        <>
          <dl>
            <div className="stat-row">
              <dt>{tt('sidebar.hitDps')}</dt>
              <dd>{formatStatValue(mainSkill.hit_dps, 'float2')}</dd>
            </div>
            {mainSkill.dot_dps > 0 && (
              <div className="stat-row">
                <dt>{tt('sidebar.dotDps')}</dt>
                <dd>{formatStatValue(mainSkill.dot_dps, 'float2')}</dd>
              </div>
            )}
            <div className="stat-row">
              <dt>{tt('sidebar.combinedDps')}</dt>
              <dd style={{ color: 'var(--accent-gold-bright)' }}>
                {formatStatValue(mainSkill.combined_dps, 'float2')}
              </dd>
            </div>
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
    <aside className="stat-sidebar" aria-label="Character stats">
      <MainSkillSection session={session} lang={lang} />
      {STAT_SECTIONS.map((section) => {
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
                      {values ? formatStatValue(values.get(row.id) ?? null, row.format) : '—'}
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
