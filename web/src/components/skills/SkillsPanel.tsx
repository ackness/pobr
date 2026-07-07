import { useEffect, useMemo, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { GemCatalogEntry, SocketGroupInput } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import { GemPicker, gemDisplayName } from './GemPicker';
import './skills.css';

interface Props {
  session: BuildSession;
  lang: Lang;
}

/** `ExplosiveGrenadePlayer` → `Explosive Grenade`（目录查不到时的展示名退化）。 */
export function prettySkillId(id: string): string {
  return id
    .replace(/Player(Two)?$/, '')
    .replace(/^Support/, '')
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2');
}

export function SkillsPanel({ session, lang }: Props) {
  const tt = bindT(lang);
  const [catalog, setCatalog] = useState<GemCatalogEntry[]>([]);
  useEffect(() => {
    getBackend()
      .then((b) => b.gemCatalog())
      .then(setCatalog)
      .catch(() => setCatalog([]));
  }, []);

  const byId = useMemo(() => new Map(catalog.map((e) => [e.skill_id, e])), [catalog]);
  const actives = useMemo(() => catalog.filter((e) => !e.is_support), [catalog]);
  const supports = useMemo(() => catalog.filter((e) => e.is_support), [catalog]);
  const gemName = (skillId: string) => {
    const entry = byId.get(skillId);
    return entry ? gemDisplayName(entry, lang) : prettySkillId(skillId);
  };

  const groups = session.socketGroups;
  const mainIndex = session.calcParams.main_socket_group ?? session.build?.main_socket_group ?? 0;

  const updateGroup = (idx: number, patch: Partial<SocketGroupInput>) => {
    session.setSocketGroups(groups.map((g, i) => (i === idx ? { ...g, ...patch } : g)));
  };

  return (
    <section aria-labelledby="skills-heading">
      <h2 id="skills-heading" className="panel-heading">
        {tt('skills.title')}
      </h2>
      <div className="skills-toolbar">
        <GemPicker
          entries={actives}
          placeholder={tt('skills.addPlaceholder')}
          disabled={session.busy || catalog.length === 0}
          lang={lang}
          onPick={(skillId) =>
            session.setSocketGroups([
              ...groups,
              { enabled: true, gems: [{ skill_id: skillId, level: 20, quality: 0 }] },
            ])
          }
        />
        <span className="skills-hint">{tt('skills.hint')}</span>
      </div>
      {groups.length === 0 && <p className="skills-hint">{tt('skills.empty')}</p>}
      <div className="skill-groups">
        {groups.map((group, idx) => {
          const isMain = idx === mainIndex;
          const [active] = group.gems;
          return (
            <div
              key={idx}
              className={`skill-group${isMain ? ' is-main' : ''}${group.enabled ? '' : ' is-disabled'}`}
            >
              <div className="skill-group-header">
                <button
                  className="skill-group-title"
                  aria-pressed={isMain}
                  disabled={session.busy}
                  title={tt('skills.setMain')}
                  onClick={() => session.updateParams({ main_socket_group: idx })}
                >
                  <span className="skill-group-name">
                    {active ? gemName(active.skill_id) : tt('skills.emptyGroup')}
                  </span>
                  {isMain && <span className="skill-group-main">{tt('skills.main')}</span>}
                </button>
                <label className="skill-group-toggle">
                  <input
                    type="checkbox"
                    checked={group.enabled}
                    disabled={session.busy}
                    onChange={(e) => updateGroup(idx, { enabled: e.target.checked })}
                  />
                  {tt('skills.enabled')}
                </label>
                <button
                  className="skill-remove"
                  disabled={session.busy}
                  title={tt('skills.removeGroup')}
                  onClick={() => session.setSocketGroups(groups.filter((_, i) => i !== idx))}
                >
                  ×
                </button>
              </div>
              <ul className="skill-gems">
                {group.gems.map((gem, gemIdx) => (
                  <li key={gemIdx} className={`skill-gem${gemIdx === 0 ? ' is-active' : ''}`}>
                    <span className="gem-name">{gemName(gem.skill_id)}</span>
                    <span className="gem-controls">
                      <input
                        type="number"
                        min={1}
                        max={40}
                        value={gem.level}
                        disabled={session.busy}
                        aria-label={tt('skills.level')}
                        onChange={(e) => {
                          const level = Number(e.target.value);
                          if (!Number.isInteger(level) || level < 1) return;
                          updateGroup(idx, {
                            gems: group.gems.map((g, i) => (i === gemIdx ? { ...g, level } : g)),
                          });
                        }}
                      />
                      <input
                        type="number"
                        min={0}
                        max={23}
                        value={gem.quality}
                        disabled={session.busy}
                        aria-label={tt('skills.quality')}
                        onChange={(e) => {
                          const quality = Number(e.target.value);
                          if (!Number.isInteger(quality) || quality < 0) return;
                          updateGroup(idx, {
                            gems: group.gems.map((g, i) => (i === gemIdx ? { ...g, quality } : g)),
                          });
                        }}
                      />
                      <button
                        className="skill-remove"
                        disabled={session.busy}
                        title={tt('skills.removeGem')}
                        onClick={() =>
                          updateGroup(idx, { gems: group.gems.filter((_, i) => i !== gemIdx) })
                        }
                      >
                        ×
                      </button>
                    </span>
                  </li>
                ))}
              </ul>
              <div className="skill-group-picker">
                <GemPicker
                  entries={supports}
                  placeholder={tt('skills.addSupport')}
                  disabled={session.busy || catalog.length === 0}
                  lang={lang}
                  onPick={(skillId) =>
                    updateGroup(idx, {
                      gems: [...group.gems, { skill_id: skillId, level: 20, quality: 0 }],
                    })
                  }
                />
              </div>
            </div>
          );
        })}
      </div>
    </section>
  );
}
