import type { BuildSession } from '../../hooks/useBuildSession';
import type { Lang } from '../../lib/statDisplay';
import './skills.css';

interface Props {
  session: BuildSession;
  lang: Lang;
}

/** `ExplosiveGrenadePlayer` → `Explosive Grenade`（无数据表时的展示名退化）。 */
export function prettySkillId(id: string): string {
  return id
    .replace(/Player(Two)?$/, '')
    .replace(/^Support/, '')
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2');
}

export function SkillsPanel({ session, lang }: Props) {
  const zh = lang === 'zh-TW';
  const build = session.build;
  if (!build || build.socket_groups.length === 0) {
    return (
      <section aria-labelledby="skills-heading">
        <h2 id="skills-heading" className="panel-heading">
          {zh ? '技能組' : 'Socket Groups'}
        </h2>
        <p className="skills-hint">
          {zh
            ? '目前沒有技能組。匯入 build code 可帶入技能；手動添加技能是後續版本的編輯器功能。'
            : 'No socket groups yet. Import a build code to bring skills in; a manual skill editor is planned for a later slice.'}
        </p>
      </section>
    );
  }
  const mainIndex = session.calcParams.main_socket_group ?? build.main_socket_group ?? 0;

  return (
    <section aria-labelledby="skills-heading">
      <h2 id="skills-heading" className="panel-heading">
        {zh ? '技能組' : 'Socket Groups'}
      </h2>
      <p className="skills-hint">
        {zh ? '點擊技能組設為主技能並重算。' : 'Click a group to set it as the main skill and recalculate.'}
      </p>
      <div className="skill-groups">
        {build.socket_groups.map((group, idx) => {
          const isMain = idx === mainIndex;
          const [active, ...supports] = group.gems;
          return (
            <button
              key={idx}
              className={`skill-group${isMain ? ' is-main' : ''}${group.enabled ? '' : ' is-disabled'}`}
              aria-pressed={isMain}
              disabled={session.busy}
              onClick={() => session.updateParams({ main_socket_group: idx })}
            >
              <header className="skill-group-header">
                <span className="skill-group-name">
                  {active ? prettySkillId(active.skill_id) : zh ? '（空組）' : '(empty)'}
                </span>
                {group.slot && <span className="skill-group-slot">{group.slot}</span>}
                {isMain && <span className="skill-group-main">{zh ? '主技能' : 'MAIN'}</span>}
                {!group.enabled && <span className="skill-group-off">{zh ? '停用' : 'OFF'}</span>}
              </header>
              <ul className="skill-gems">
                {active && (
                  <li className="skill-gem is-active">
                    {prettySkillId(active.skill_id)}
                    <span className="gem-level">
                      {active.level}/{active.quality}
                    </span>
                  </li>
                )}
                {supports.map((gem, i) => (
                  <li key={i} className="skill-gem">
                    {prettySkillId(gem.skill_id)}
                    <span className="gem-level">
                      {gem.level}/{gem.quality}
                    </span>
                  </li>
                ))}
              </ul>
            </button>
          );
        })}
      </div>
    </section>
  );
}
