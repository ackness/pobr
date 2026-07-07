import { useEffect, useMemo, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { GemCatalogEntry, SocketGroupInput } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import type { Lang } from '../../lib/statDisplay';
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

/** 宝石搜索选择框（datalist；按显示名匹配，命中即回调 skill_id 并清空）。 */
function GemPicker({
  id,
  entries,
  placeholder,
  disabled,
  onPick,
}: {
  id: string;
  entries: GemCatalogEntry[];
  placeholder: string;
  disabled: boolean;
  onPick: (skillId: string) => void;
}) {
  const [text, setText] = useState('');
  const byName = useMemo(() => new Map(entries.map((e) => [e.name, e.skill_id])), [entries]);
  return (
    <>
      <input
        list={id}
        value={text}
        placeholder={placeholder}
        disabled={disabled}
        aria-label={placeholder}
        onChange={(e) => {
          const value = e.target.value;
          const skillId = byName.get(value);
          if (skillId) {
            onPick(skillId);
            setText('');
          } else {
            setText(value);
          }
        }}
      />
      <datalist id={id}>
        {entries.map((e) => (
          <option key={e.skill_id} value={e.name} />
        ))}
      </datalist>
    </>
  );
}

export function SkillsPanel({ session, lang }: Props) {
  const zh = lang === 'zh-TW';
  const [catalog, setCatalog] = useState<GemCatalogEntry[]>([]);
  useEffect(() => {
    getBackend()
      .then((b) => b.gemCatalog())
      .then(setCatalog)
      .catch(() => setCatalog([]));
  }, []);

  const nameById = useMemo(
    () => new Map(catalog.map((e) => [e.skill_id, e.name])),
    [catalog],
  );
  const actives = useMemo(() => catalog.filter((e) => !e.is_support), [catalog]);
  const supports = useMemo(() => catalog.filter((e) => e.is_support), [catalog]);
  const gemName = (skillId: string) => nameById.get(skillId) ?? prettySkillId(skillId);

  const groups = session.socketGroups;
  const mainIndex = session.calcParams.main_socket_group ?? session.build?.main_socket_group ?? 0;

  const updateGroup = (idx: number, patch: Partial<SocketGroupInput>) => {
    session.setSocketGroups(groups.map((g, i) => (i === idx ? { ...g, ...patch } : g)));
  };

  return (
    <section aria-labelledby="skills-heading">
      <h2 id="skills-heading" className="panel-heading">
        {zh ? '技能組' : 'Socket Groups'}
      </h2>
      <div className="skills-toolbar">
        <GemPicker
          id="picker-new-group"
          entries={actives}
          placeholder={zh ? '搜尋主動技能以新建組…' : 'Search an active gem to add a group…'}
          disabled={session.busy || catalog.length === 0}
          onPick={(skillId) =>
            session.setSocketGroups([
              ...groups,
              { enabled: true, gems: [{ skill_id: skillId, level: 20, quality: 0 }] },
            ])
          }
        />
        <span className="skills-hint">
          {zh ? '點組標題設為主技能；等級/品質即改即算。' : 'Click a group title to make it the main skill; level/quality edits recalc live.'}
        </span>
      </div>
      {groups.length === 0 && (
        <p className="skills-hint">
          {zh ? '尚無技能組——用上方搜尋框添加，或匯入 build code。' : 'No socket groups yet — add one with the search box above, or import a build code.'}
        </p>
      )}
      <div className="skill-groups">
        {groups.map((group, idx) => {
          const isMain = idx === mainIndex;
          const [active, ...rest] = group.gems;
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
                  title={zh ? '設為主技能' : 'Set as main skill'}
                  onClick={() => session.updateParams({ main_socket_group: idx })}
                >
                  <span className="skill-group-name">
                    {active ? gemName(active.skill_id) : zh ? '（空組）' : '(empty)'}
                  </span>
                  {isMain && <span className="skill-group-main">{zh ? '主技能' : 'MAIN'}</span>}
                </button>
                <label className="skill-group-toggle">
                  <input
                    type="checkbox"
                    checked={group.enabled}
                    disabled={session.busy}
                    onChange={(e) => updateGroup(idx, { enabled: e.target.checked })}
                  />
                  {zh ? '啟用' : 'on'}
                </label>
                <button
                  className="skill-remove"
                  disabled={session.busy}
                  title={zh ? '刪除組' : 'Remove group'}
                  onClick={() => session.setSocketGroups(groups.filter((_, i) => i !== idx))}
                >
                  ×
                </button>
              </div>
              <ul className="skill-gems">
                {[active, ...rest].filter(Boolean).map((gem, gemIdx) => (
                  <li key={gemIdx} className={`skill-gem${gemIdx === 0 ? ' is-active' : ''}`}>
                    <span className="gem-name">{gemName(gem.skill_id)}</span>
                    <span className="gem-controls">
                      <input
                        type="number"
                        min={1}
                        max={40}
                        value={gem.level}
                        disabled={session.busy}
                        aria-label={zh ? '等級' : 'Level'}
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
                        aria-label={zh ? '品質' : 'Quality'}
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
                        title={zh ? '移除宝石' : 'Remove gem'}
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
              <GemPicker
                id={`picker-support-${idx}`}
                entries={supports}
                placeholder={zh ? '添加輔助宝石…' : 'Add a support gem…'}
                disabled={session.busy || catalog.length === 0}
                onPick={(skillId) =>
                  updateGroup(idx, {
                    gems: [...group.gems, { skill_id: skillId, level: 20, quality: 0 }],
                  })
                }
              />
            </div>
          );
        })}
      </div>
    </section>
  );
}
