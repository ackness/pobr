import { WeaponSetControl } from '../shared/WeaponSetControl';
import { PageHeader } from '../shared/PageHeader';
import { useEffect, useMemo, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { GemCatalogEntry, SocketGroupInput } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, grantedSourceLabel, type Lang } from '../../lib/i18n';
import { GemPicker, gemDisplayName } from './GemPicker';
import { GemOptimizer } from './GemOptimizer';
import { NoteEditor } from '../shared/NoteEditor';
import { AppSelect } from '../shared/AppSelect';
import { loadTradeCatalog } from '../../lib/tradeOptimizer';
import { eligibleSupports, lineageAvailable, supportSetCompatible, usableSupportLevel, type SupportMetadata } from '../../lib/supportOptimizer';
import { supportText } from '../../lib/supportI18n';
import './skills.css';

interface Props {
  session: BuildSession;
  lang: Lang;
  focusOptimizer?: { group: number; nonce: number };
}

/** `ExplosiveGrenadePlayer` → `Explosive Grenade`（目录查不到时的展示名退化）。 */
export function prettySkillId(id: string): string {
  return id
    .replace(/Player(Two)?$/, '')
    .replace(/^Support/, '')
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2');
}

export function SkillsPanel({ session, lang, focusOptimizer }: Props) {
  const tt = bindT(lang);
  const [catalog, setCatalog] = useState<GemCatalogEntry[]>([]);
  const [tradeGems, setTradeGems] = useState<SupportMetadata[]>([]);
  const [catalogError, setCatalogError] = useState(false);
  useEffect(() => {
    let active = true;
    Promise.all([getBackend().then(backend => backend.gemCatalog()), loadTradeCatalog()])
      .then(([entries, trade]) => {
        if (!active) return;
        const metadata = new Map(trade.gems?.map(gem => [gem.skill_id, gem as SupportMetadata]));
        setTradeGems(trade.gems ?? []);
        setCatalog(entries.map(entry => ({ ...entry, is_lineage: metadata.get(entry.skill_id)?.is_lineage ?? entry.is_lineage })));
      }).catch(() => { if (active) setCatalogError(true); });
    return () => { active = false; };
  }, []);

  const byId = useMemo(() => new Map(catalog.map((e) => [e.skill_id, e])), [catalog]);
  const actives = useMemo(() => catalog.filter((e) => !e.is_support), [catalog]);
  const supports = useMemo(() => catalog.filter((e) => e.is_support), [catalog]);
  const tradeById = useMemo(() => new Map(tradeGems.map(gem => [gem.skill_id, gem])), [tradeGems]);
  const gemName = (skillId: string) => {
    const entry = byId.get(skillId);
    return entry ? gemDisplayName(entry, lang) : prettySkillId(skillId);
  };

  const groups = session.socketGroups;
  const mainIndex = session.calcParams.main_socket_group ?? session.build?.main_socket_group ?? 0;
  // 手风琴：同一时刻只展开一个组编辑，其余收成单行摘要。
  const [openIdx, setOpenIdx] = useState<number | null>(null);
  useEffect(() => {
    if (!focusOptimizer) return;
    setOpenIdx(focusOptimizer.group);
  }, [focusOptimizer]);

  const updateGroup = (idx: number, patch: Partial<SocketGroupInput>) => {
    session.setSocketGroups(groups.map((g, i) => (i === idx ? { ...g, ...patch } : g)));
  };

  return (
    <section className="ui-page skills-page" aria-labelledby="skills-heading">
      <PageHeader id="skills-heading" title={tt('skills.title')} description={tt('skills.hint')}>
        <WeaponSetControl session={session} lang={lang} />
      </PageHeader>
      <div className="skills-toolbar">
        <GemPicker
          entries={actives}
          placeholder={tt('skills.addPlaceholder')}
          disabled={session.busy || catalog.length === 0}
          lang={lang}
          onPick={(skillId) => {
            session.setSocketGroups([
              ...groups,
              { enabled: true, gems: [{ skill_id: skillId, level: 20, quality: 0 }] },
            ]);
            setOpenIdx(groups.length);
          }}
        />

      </div>
      {catalogError && <p className="opt-error">{supportText(lang, 'catalogError')}</p>}
      {!catalogError && !catalog.length && <p className="skills-hint">{supportText(lang, 'loading')}</p>}
      {groups.length === 0 && <p className="skills-hint">{tt('skills.empty')}</p>}
      <SkillSets session={session} lang={lang} />
      <div className="skill-groups">
        {groups.map((group, idx) => {
          const isMain = idx === mainIndex;
          const isOpen = idx === openIdx;
          const activeGems = group.gems.filter(gem => !byId.get(gem.skill_id)?.is_support);
          const selectedActive = activeGems[Math.min(Math.max((group.main_active_skill ?? 1) - 1, 0), activeGems.length - 1)];
          const active = session.calc?.main_skill?.group_index === idx
            ? group.gems.find(gem => gem.skill_id === session.calc?.main_skill?.skill_id) ?? selectedActive
            : selectedActive;
          const supportGems = group.gems.filter(gem => gem !== active);
          const currentSupports = group.gems.filter(gem => tradeById.get(gem.skill_id)?.is_support);
          const eligibleIds = new Set((isOpen ? eligibleSupports(group, tradeGems, session.character?.level ?? 1).gems : [])
            .filter(gem => lineageAvailable(gem, groups, idx) && supportSetCompatible(group, [...currentSupports,
              { skill_id: gem.skill_id, level: usableSupportLevel(gem, session.character?.level ?? 1), quality: 0 }], tradeGems))
            .map(gem => gem.skill_id));
          const availableSupports = supports.filter(gem => eligibleIds.has(gem.skill_id));
          const optimizerSkillKey = JSON.stringify(group.gems
            .filter(gem => !tradeById.get(gem.skill_id)?.is_support).map(gem => gem.skill_id).sort());
          return (
            <div
              key={idx}
              className={`skill-group${isMain ? ' is-main' : ''}${group.enabled ? '' : ' is-disabled'}`}
            >
              <div className="skill-group-header">
                <button
                  className={`skill-main-toggle${isMain ? ' is-on' : ''}`}
                  aria-pressed={isMain}
                  disabled={session.busy}
                  title={tt('skills.setMain')}
                  onClick={() => session.updateParams({ main_socket_group: idx })}
                >
                  ★
                </button>
                <button
                  className="skill-group-title"
                  aria-expanded={isOpen}
                  onClick={() => setOpenIdx(isOpen ? null : idx)}
                >
                  <span className="row-caret" aria-hidden>
                    ▸
                  </span>
                  <span className="skill-group-name">
                    {active ? gemName(active.skill_id) : tt('skills.emptyGroup')}
                    {Boolean(session.annotations[`skill:${idx}`]?.trim()) && (
                      <span className="note-dot" aria-hidden />
                    )}
                  </span>
                  {group.weapon_set && <span className="ui-badge">{tt('weapons.set')} {group.weapon_set}</span>}
                  {isMain && <span className="skill-group-main">{tt('skills.main')}</span>}
                  {grantedSourceLabel(lang, group.source) && (
                    <span className="granted-badge">
                      {grantedSourceLabel(lang, group.source)}
                    </span>
                  )}
                  {!isOpen && supportGems.length > 0 && (
                    <span className="skill-group-supports">
                      {supportGems.map((g) => `${gemName(g.skill_id)}${byId.get(g.skill_id)?.is_lineage ? ` [${tt('picker.lineage')}]` : ''}`).join(' · ')}
                    </span>
                  )}
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
                  onClick={() => {
                    session.removeSocketGroup(idx);
                    setOpenIdx(null);
                  }}
                >
                  ×
                </button>
              </div>
              {isOpen && (
              <div className="skill-gem-editor">
              <div className="skill-weapon-binding"><label>{tt('weapons.binding')}</label>
                <AppSelect ariaLabel={tt('weapons.binding')} value={String(group.weapon_set ?? 0)}
                  options={[{ value: '0', label: tt('weapons.both') }, ...[1, 2].map(set => ({ value: String(set), label: `${tt('weapons.set')} ${set}` }))]}
                  disabled={session.busy} onChange={value => updateGroup(idx, { weapon_set: value === '0' ? null : Number(value) as 1 | 2 })} />
              </div>
              <div className="gem-column-labels"><span>{tt('calcs.skill')}</span><span>{tt('skills.level')}</span><span>{tt('skills.quality')}</span><span /></div>
              <ul className="skill-gems">
                {group.gems.map((gem, gemIdx) => (
                  <li key={gemIdx} className={`skill-gem${gemIdx === 0 ? ' is-active' : ''}`}>
                    <span className="gem-name">{gemName(gem.skill_id)}
                      {byId.get(gem.skill_id)?.is_lineage && <span className="gem-lineage-badge">{tt('picker.lineage')}</span>}
                    </span>
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
                        onClick={() => {
                          const removedActive = activeGems.indexOf(gem);
                          const selected = Math.min(Math.max((group.main_active_skill ?? 1) - 1, 0), activeGems.length - 1);
                          const nextSelected = removedActive >= 0 && removedActive <= selected
                            ? Math.max(0, selected - 1) : selected;
                          updateGroup(idx, {
                            gems: group.gems.filter((_, i) => i !== gemIdx),
                            main_active_skill: group.main_active_skill == null ? undefined : nextSelected + 1,
                          });
                        }}
                      >
                        ×
                      </button>
                    </span>
                  </li>
                ))}
              </ul>
              </div>
              )}
              {isOpen && (
                <div className="skill-group-picker">
                  <GemPicker
                    entries={availableSupports}
                    placeholder={tt('skills.addSupport')}
                    disabled={session.busy || catalog.length === 0 || currentSupports.length >= 5}
                    lang={lang}
                    onPick={(skillId) =>
                      updateGroup(idx, {
                        gems: [...group.gems, { skill_id: skillId,
                          level: usableSupportLevel(tradeById.get(skillId)!, session.character?.level ?? 1), quality: 0 }],
                      })
                    }
                  />
                </div>
              )}
              {isOpen && (
                <GemOptimizer
                  key={optimizerSkillKey}
                  skillKey={optimizerSkillKey}
                  session={session}
                  lang={lang}
                  groupIndex={idx}
                  catalog={tradeGems}
                  gemName={gemName}
                  focusNonce={focusOptimizer?.group === idx ? focusOptimizer.nonce : undefined}
                />
              )}
              {isOpen && (
                <div className="skill-group-note">
                  <NoteEditor
                    value={session.annotations[`skill:${idx}`] ?? ''}
                    onCommit={(text) => session.setAnnotation(`skill:${idx}`, text)}
                    lang={lang}
                  />
                </div>
              )}
            </div>
          );
        })}
      </div>
    </section>
  );
}

/** 技能组套装：整套保存/一键切换（切换后侧边栏即见差异）。 */
function SkillSets({ session, lang }: { session: BuildSession; lang: Lang }) {
  const tt = bindT(lang);
  const [name, setName] = useState('');
  return (
    <div className="skill-sets" role="group" aria-label={tt('sets.title')}>
      <span className="skill-sets-title">{tt('sets.title')}</span>
      {session.library.skillSets.map((set) => (
        <span key={set.id} className="skill-set-chip">
          {set.name}（{set.groups.length}）
          <button disabled={session.busy} onClick={() => session.applySkillSet(set.id)}>
            {tt('sets.apply')}
          </button>
          <button className="skill-remove" onClick={() => session.removeSkillSet(set.id)}>
            ×
          </button>
        </span>
      ))}
      <input
        placeholder={tt('sets.namePlaceholder')}
        value={name}
        onChange={(e) => setName(e.target.value)}
        aria-label={tt('sets.namePlaceholder')}
      />
      <button
        disabled={!name.trim() || session.socketGroups.length === 0}
        onClick={() => {
          session.saveSkillSet(name.trim());
          setName('');
        }}
      >
        {tt('sets.save')}
      </button>
    </div>
  );
}
