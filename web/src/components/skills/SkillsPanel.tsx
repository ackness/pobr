import { WeaponSetControl } from '../shared/WeaponSetControl';
import { PageHeader } from '../shared/PageHeader';
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { GemCatalogEntry, SocketGroupInput } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, grantedSourceLabel, type Lang } from '../../lib/i18n';
import { GemPicker } from './GemPicker';
import { skillDisplayName } from '../../lib/skillNames';
import { activeSkillEffects, removeSkillGem, withGemCorrupted, withGemEnabled } from '../../lib/skillSelection';
export { prettySkillId } from '../../lib/skillNames';
import { GemOptimizer } from './GemOptimizer';
import { NoteEditor } from '../shared/NoteEditor';
import { AppSelect } from '../shared/AppSelect';
import { loadTradeCatalog } from '../../lib/tradeOptimizer';
import { eligibleSupports, lineageAvailable, supportSetsCompatible, usableSupportLevel, type SupportMetadata } from '../../lib/supportOptimizer';
import { supportText } from '../../lib/supportI18n';
import { MinionSelection } from './MinionSelection';
import { SkillFormControls } from './SkillFormControls';
import './skills.css';

interface Props {
  session: BuildSession;
  lang: Lang;
  active: boolean;
  focusOptimizer?: { group: number; nonce: number };
}

export function SkillsPanel({ session, lang, active, focusOptimizer }: Props) {
  const tt = bindT(lang);
  const [catalog, setCatalog] = useState<GemCatalogEntry[]>([]);
  const [tradeGems, setTradeGems] = useState<SupportMetadata[]>([]);
  const [catalogError, setCatalogError] = useState(false);
  useEffect(() => {
    let active = true;
    Promise.all([getBackend().then(backend => backend.gemCatalog()), loadTradeCatalog()])
      .then(([entries, trade]) => {
        if (!active) return;
        const metadata = new Map(trade.gems?.map(gem => [gem.skill_id, gem]));
        const effects = new Map(entries.map(entry => [entry.skill_id, entry.active_effect_ids]));
        setTradeGems((trade.gems ?? []).flatMap(gem => effects.has(gem.skill_id)
          ? [{ ...gem, active_effect_ids: effects.get(gem.skill_id)! }] : []));
        setCatalog(entries.map(entry => ({ ...entry, is_lineage: metadata.get(entry.skill_id)?.is_lineage ?? entry.is_lineage })));
      }).catch(() => { if (active) setCatalogError(true); });
    return () => { active = false; };
  }, []);

  const byId = useMemo(() => new Map(catalog.map((e) => [e.skill_id, e])), [catalog]);
  const actives = useMemo(() => catalog.filter((e) => !e.is_support), [catalog]);
  const supports = useMemo(() => catalog.filter((e) => e.is_support), [catalog]);
  const tradeById = useMemo(() => new Map(tradeGems.map(gem => [gem.skill_id, gem])), [tradeGems]);
  const gemName = (skillId: string) => skillDisplayName(skillId, lang, byId);
  const gemLabel = (gem: SocketGroupInput['gems'][number]) => gem.skill_id ? gemName(gem.skill_id)
    : gem.name_spec ? `${gem.name_spec} · ${tt('skills.unresolved')}` : tt('skills.unresolved');

  const groups = session.socketGroups;
  const mainIndex = session.calcParams.main_socket_group;
  // Retain only the selected workspace; group ordinals cannot own it after compaction.
  const groupRefs = useRef<Array<HTMLDivElement | null>>([]);
  const scrollFrame = useRef<number | null>(null);
  const [openGroup, setOpenGroup] = useState<{ index: number; revision: number; focusNonce?: number } | null>(null);
  const openIdx = openGroup?.revision === session.socketGroupRevision ? openGroup.index : null;
  const setOpenIdx = (index: number | null, focusNonce?: number) => setOpenGroup(index === null
    ? null : { index, revision: session.socketGroupRevision, focusNonce });
  const consumedFocus = useRef<number | undefined>(undefined);
  useLayoutEffect(() => {
    if (!active || !focusOptimizer || consumedFocus.current === focusOptimizer.nonce) return;
    consumedFocus.current = focusOptimizer.nonce;
    if (groups[focusOptimizer.group]) setOpenIdx(focusOptimizer.group, focusOptimizer.nonce);
  }, [active, focusOptimizer, session.socketGroupRevision]);
  useLayoutEffect(() => {
    if (openGroup && openIdx === null) setOpenGroup(null);
  }, [session.socketGroupRevision, openIdx]);
  useLayoutEffect(() => () => {
    if (scrollFrame.current !== null) cancelAnimationFrame(scrollFrame.current);
    scrollFrame.current = null;
  }, [active]);

  const pickerGroup = openIdx === null ? undefined : groups[openIdx];
  const supportPickerKey = JSON.stringify([session.socketGroupRevision, session.dataVersion, session.treeVersion,
    openIdx, groups, session.character?.level]);
  const [compatibleSupports, setCompatibleSupports] = useState<{ key: string; catalog: SupportMetadata[]; ids: Set<string> }>();
  useLayoutEffect(() => {
    if (!active || !pickerGroup || openIdx === null || !tradeGems.length) return;
    if (compatibleSupports?.key === supportPickerKey && compatibleSupports.catalog === tradeGems) return;
    const controller = new AbortController();
    const currentSupports = pickerGroup.gems.filter(gem => tradeById.get(gem.skill_id)?.is_support);
    const pool = eligibleSupports(pickerGroup, tradeGems, session.character?.level ?? 1).gems
      .filter(gem => lineageAvailable(gem, groups, openIdx));
    const candidates = pool.map(gem => [...currentSupports,
      { skill_id: gem.skill_id, level: usableSupportLevel(gem, session.character?.level ?? 1), quality: 0 }]);
    supportSetsCompatible(pickerGroup, candidates, tradeGems, controller.signal).then(compatible => {
      if (controller.signal.aborted) return;
      setCompatibleSupports({ key: supportPickerKey, catalog: tradeGems,
        ids: new Set(pool.filter((_, index) => compatible[index]).map(gem => gem.skill_id)) });
    }).catch(() => { if (!controller.signal.aborted) setCatalogError(true); });
    return () => controller.abort();
  }, [active, supportPickerKey, tradeGems]);

  const updateGroup = (idx: number, patch: Partial<SocketGroupInput>) => {
    session.setSocketGroups(groups.map((g, i) => (i === idx ? { ...g, ...patch } : g)));
  };

  return (
    <section className="ui-page skills-page" aria-labelledby="skills-heading" hidden={!active}>
      {active && <PageHeader id="skills-heading" title={tt('skills.title')} description={tt('skills.hint')}>
        <WeaponSetControl session={session} lang={lang} />
      </PageHeader>}
      {active && <div className="skills-toolbar">
        {mainIndex !== undefined && groups[mainIndex] && <button onClick={() => {
          setOpenIdx(mainIndex);
          if (scrollFrame.current !== null) cancelAnimationFrame(scrollFrame.current);
          scrollFrame.current = requestAnimationFrame(() => {
            scrollFrame.current = null;
            groupRefs.current[mainIndex]?.scrollIntoView({ block: 'start' });
          });
        }}>{tt('skills.editMain')} · {gemName(session.calc?.main_skill?.skill_id ?? groups[mainIndex].gems[0]?.skill_id ?? '')}</button>}
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

      </div>}
      {catalogError && <p className="opt-error">{supportText(lang, 'catalogError')}</p>}
      {!catalogError && !catalog.length && <p className="skills-hint">{supportText(lang, 'loading')}</p>}
      {groups.length === 0 && <p className="skills-hint">{tt('skills.empty')}</p>}
      {active && <SkillSets session={session} lang={lang} />}
      <div className="skill-groups">
        {groups.map((group, idx) => {
          const isMain = idx === mainIndex;
          const isOpen = idx === openIdx;
          const effects = activeSkillEffects(group, byId);
          const selectedEffect = effects[(group.main_active_skill ?? 1) - 1];
          const selectedId = selectedEffect?.skillId;
          const activeGem = selectedEffect ? group.gems[selectedEffect.gemIndex] : undefined;
          const supportGems = group.gems.filter(gem => gem !== activeGem);
          const currentSupports = group.gems.filter(gem => tradeById.get(gem.skill_id)?.is_support);
          const availableSupports = supports.filter(gem => isOpen && compatibleSupports?.key === supportPickerKey
            && compatibleSupports.catalog === tradeGems && compatibleSupports.ids.has(gem.skill_id));
          const optimizerSkillKey = JSON.stringify(group.gems
            .filter(gem => !tradeById.get(gem.skill_id)?.is_support).map(gem => gem.skill_id).sort());
          return (
            <div
              key={idx}
              ref={element => { groupRefs.current[idx] = element; }}
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
                    {selectedId ? gemName(selectedId) : tt('skills.emptyGroup')}
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
                      {supportGems.map((g) => `${gemLabel(g)}${byId.get(g.skill_id)?.is_lineage ? ` [${tt('picker.lineage')}]` : ''}`).join(' · ')}
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
              {active && isOpen && (
              <div className="skill-gem-editor">
              <div className="skill-weapon-binding"><label>{tt('weapons.binding')}</label>
                <AppSelect ariaLabel={tt('weapons.binding')} value={String(group.weapon_set ?? 0)}
                  options={[{ value: '0', label: tt('weapons.both') }, ...[1, 2].map(set => ({ value: String(set), label: `${tt('weapons.set')} ${set}` }))]}
                  disabled={session.busy} onChange={value => updateGroup(idx, { weapon_set: value === '0' ? null : Number(value) as 1 | 2 })} />
              </div>
              <div className="skill-full-dps-controls">
                <label><input type="checkbox" checked={group.include_in_full_dps ?? false} disabled={session.busy}
                  onChange={event => updateGroup(idx, { include_in_full_dps: event.target.checked })} />{tt('skills.fullDps')}</label>
                <label>{tt('skills.groupCount')}<input type="number" min={0} step="any" aria-label={tt('skills.groupCount')}
                  value={group.group_count ?? ''} disabled={session.busy} onChange={event => {
                    const value = event.target.value;
                    if (!value || Number.isFinite(Number(value)) && Number(value) >= 0) updateGroup(idx, { group_count: value ? Number(value) : null });
                  }} /></label>
                <p className="skills-hint">{tt('skills.fullDpsHint')}</p>
              </div>
              {effects.length > 1 && <label className="skill-effect-control">{tt('skills.effect')}
                <AppSelect ariaLabel={tt('skills.effect')} value={String(group.main_active_skill ?? 1)} disabled={session.busy}
                  options={effects.map((effect, index) => ({ value: String(index + 1), label: `${index + 1} · ${gemName(effect.skillId)}` }))}
                  onChange={value => updateGroup(idx, { main_active_skill: Number(value) })} />
              </label>}
              <SkillFormControls session={session} lang={lang} groupIndex={idx} catalog={byId} />
              <div className="gem-column-labels"><span>{tt('calcs.skill')}</span><span>{tt('skills.level')}</span><span>{tt('skills.quality')}</span><span>{tt('skills.corrupted')}</span><span /></div>
              <ul className="skill-gems">
                {group.gems.map((gem, gemIdx) => (
                  <li key={gemIdx} className={`skill-gem${gemIdx === 0 ? ' is-active' : ''}${gem.enabled === false ? ' is-disabled' : ''}`}>
                    <span className="gem-name" title={gem.skill_id === '' ? tt('skills.unresolvedHint') : undefined}>
                      {gemLabel(gem)}
                      {byId.get(gem.skill_id)?.is_lineage && <span className="gem-lineage-badge">{tt('picker.lineage')}</span>}
                    </span>
                    <span className="gem-controls">
                      <label className="gem-enabled"><input type="checkbox" aria-label={tt('skills.gemEnabled')}
                        checked={gem.enabled !== false} disabled={session.busy}
                        onChange={event => updateGroup(idx, withGemEnabled(group, gemIdx, event.target.checked, byId))} />{tt('skills.gemEnabled')}</label>
                      <label className="gem-count">{tt('skills.count')}<input type="number" min={0} step="any" value={gem.count ?? 1}
                        aria-label={tt('skills.gemCount')} disabled={session.busy} onChange={event => {
                          const count = Number(event.target.value);
                          if (Number.isFinite(count) && count >= 0) updateGroup(idx, { gems: group.gems.map((item, index) => index === gemIdx ? { ...item, count } : item) });
                        }} /></label>
                      <label className="gem-number">{tt('skills.level')}<input
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
                      /></label>
                      <label className="gem-number">{tt('skills.quality')}<input
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
                      /></label>
                      <label className="gem-corrupted" title={tt('skills.corruptedHint')}>
                        {tt('skills.corrupted')}
                        <input
                          type="checkbox"
                          checked={gem.corrupted === true}
                          disabled={session.busy}
                          aria-label={tt('skills.corrupted')}
                          onChange={(e) => updateGroup(idx, {
                            gems: group.gems.map((g, i) => (i === gemIdx ? withGemCorrupted(g, e.target.checked) : g)),
                          })}
                        />
                      </label>
                      <button
                        className="skill-remove"
                        disabled={session.busy || catalog.length === 0}
                        title={tt('skills.removeGem')}
                        onClick={() => updateGroup(idx, removeSkillGem(group, gemIdx, byId))}
                      >
                        ×
                      </button>
                    </span>
                  </li>
                ))}
              </ul>
              {group.gems.map((gem, gemIndex) => <MinionSelection key={gemIndex} session={session} lang={lang}
                groupIndex={idx} gemIndex={gemIndex} gemName={gemLabel(gem)} />)}
              </div>
              )}
              {active && isOpen && (
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
                  key={`${session.socketGroupRevision}:${optimizerSkillKey}`}
                  skillKey={optimizerSkillKey}
                  session={session}
                  lang={lang}
                  active={active}
                  groupIndex={idx}
                  catalog={tradeGems}
                  gemName={gemName}
                  focusNonce={openGroup?.focusNonce}
                />
              )}
              {active && isOpen && (
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
          {lang === 'en-US' ? set.name : session.classNames.ascendancies[set.name] ?? session.classNames.classes[set.name] ?? set.name}（{set.groups.length}）
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
