import { localizedName } from '../../lib/chinese';
import { AppTooltip, AppButton, AppInput } from '../shared/controls';
import { NamedSets } from '../shared/NamedSets';
import { WeaponSetControl } from '../shared/WeaponSetControl';
import { PageHeader } from '../shared/PageHeader';
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
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
import { words } from '../../app/text';
import { barShare, groupDps, topDps, type GroupDps } from '../../app/pages/skills/dps';
import { useSkillMetrics } from '../../app/pages/skills/useSkillMetrics';
import { formatStatValue } from '../../lib/statDisplay';
import './skills.css';

interface Props {
  session: BuildSession;
  lang: Lang;
  active: boolean;
  focusOptimizer?: { group: number; nonce: number };
}

export function SkillsPanel({ session, lang, active, focusOptimizer }: Props) {
  const tt = bindT(lang);
  const w = (en: string, cn: string, tw?: string) => words(lang, en, cn, tw);
  const [catalog, setCatalog] = useState<GemCatalogEntry[]>([]);
  const [tradeGems, setTradeGems] = useState<SupportMetadata[]>([]);
  const [catalogError, setCatalogError] = useState(false);
  const [showAllGroups, setShowAllGroups] = useState(false);
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
  const [openGroup, setOpenGroup] = useState<{ index: number; revision: number; focusNonce?: number } | null>(null);
  const storedIdx = openGroup?.revision === session.socketGroupRevision ? openGroup.index : null;
  // The master-detail always has a group selected: the one the player chose, else the main skill.
  const defaultIdx = mainIndex !== undefined && groups[mainIndex] ? mainIndex : groups.length ? 0 : null;
  const openIdx = storedIdx !== null && groups[storedIdx] ? storedIdx : defaultIdx;
  const setOpenIdx = (index: number | null, focusNonce?: number) => setOpenGroup(index === null
    ? null : { index, revision: session.socketGroupRevision, focusNonce });
  const consumedFocus = useRef<number | undefined>(undefined);
  useLayoutEffect(() => {
    if (!active || !focusOptimizer || consumedFocus.current === focusOptimizer.nonce) return;
    consumedFocus.current = focusOptimizer.nonce;
    if (groups[focusOptimizer.group]) setOpenIdx(focusOptimizer.group, focusOptimizer.nonce);
  }, [active, focusOptimizer, session.socketGroupRevision]);
  useLayoutEffect(() => {
    if (openGroup && storedIdx === null) setOpenGroup(null);
  }, [session.socketGroupRevision, storedIdx]);

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

  // What each group's selected skill deals on its own, for the bars beside the list.
  const dpsSnapshot = useSkillMetrics(session, active);
  const dpsRows = useMemo(() => dpsSnapshot?.revision === session.socketGroupRevision
    ? groupDps(dpsSnapshot.skills, groups) : new Map<number, GroupDps>(), [dpsSnapshot, session.socketGroupRevision, groups]);
  const dpsTop = topDps(dpsRows);

  const toolbar = active && <div className="skills-toolbar">
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
    {groups.length > 0 && <p className="skill-list-note">{w('Damage per second of each skill on its own.', '各技能单独的每秒伤害。', '各技能單獨的每秒傷害。')}</p>}
  </div>;

  return (
    <section className="ui-page skills-page" aria-labelledby="skills-heading" hidden={!active}>
      {active && <PageHeader id="skills-heading" title={tt('skills.title')} description={tt('skills.hint')}>
        <WeaponSetControl session={session} lang={lang} />
      </PageHeader>}
      {active && <NamedSets session={session} lang={lang} kind="skill" />}
      {catalogError && <p className="opt-error">{supportText(lang, 'catalogError')}</p>}
      {!catalogError && !catalog.length && <p className="skills-hint">{supportText(lang, 'loading')}</p>}
      <div className="skill-groups" data-list-expanded={showAllGroups || undefined} style={{ '--skill-rows': groups.length + 1 } as CSSProperties}>
        {groups.length > 0 ? toolbar : active && <div className="skills-empty">
          <h3>{w('Start with the skill that deals your damage', '先加入造成伤害的主技能', '先加入造成傷害的主技能')}</h3>
          <p>{w('Search for an active skill below to create the first group, or import a build from the library.', '在下方搜索一个主动技能来创建第一个技能组，或到 BD 库导入一套 BD。', '在下方搜尋一個主動技能來建立第一個技能組，或到 BD 庫匯入一套 BD。')}</p>
          {toolbar}
        </div>}
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
          const dps = dpsRows.get(idx);
          const optimizer = isOpen && (
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
          );
          const supportPicker = active && isOpen && (
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
          );
          const note = active && isOpen && (
            <div className="skill-group-note">
              <NoteEditor
                value={session.annotations[`skill:${idx}`] ?? ''}
                onCommit={(text) => session.setAnnotation(`skill:${idx}`, text)}
                lang={lang}
              />
            </div>
          );
          const weaponBinding = (
            <div className="skill-weapon-binding"><label>{tt('weapons.binding')}</label>
              <AppSelect ariaLabel={tt('weapons.binding')} value={String(group.weapon_set ?? 0)}
                options={[{ value: '0', label: tt('weapons.both') }, ...[1, 2].map(set => ({ value: String(set), label: `${tt('weapons.set')} ${set}` }))]}
                disabled={session.busy} onChange={value => updateGroup(idx, { weapon_set: value === '0' ? null : Number(value) as 1 | 2 })} />
            </div>
          );
          const fullDps = (
            <div className="skill-full-dps-controls">
              <label><AppInput type="checkbox" checked={group.include_in_full_dps ?? false} disabled={session.busy}
                onChange={event => updateGroup(idx, { include_in_full_dps: event.target.checked })} />{tt('skills.fullDps')}</label>
              <label>{tt('skills.groupCount')}<AppInput type="number" min={0} step="any" aria-label={tt('skills.groupCount')}
                value={group.group_count ?? ''} disabled={session.busy} onChange={event => {
                  const value = event.target.value;
                  if (!value || Number.isFinite(Number(value)) && Number(value) >= 0) updateGroup(idx, { group_count: value ? Number(value) : null });
                }} /></label>
              <p className="skills-hint">{tt('skills.fullDpsHint')}</p>
            </div>
          );
          const effectControl = effects.length > 1 && <label className="skill-effect-control">{tt('skills.effect')}
            <AppSelect ariaLabel={tt('skills.effect')} value={String(group.main_active_skill ?? 1)} disabled={session.busy}
              options={effects.map((effect, index) => ({ value: String(index + 1), label: `${index + 1} · ${gemName(effect.skillId)}` }))}
              onChange={value => updateGroup(idx, { main_active_skill: Number(value) })} />
          </label>;
          const gemList = (
            <ul className="skill-gems">
              {group.gems.map((gem, gemIdx) => (
                <li key={gemIdx} className={`skill-gem${gem === activeGem ? ' is-active' : ''}${gem.enabled === false ? ' is-disabled' : ''}`}
                  data-gem-kind={byId.get(gem.skill_id)?.is_support === true ? 'support' : byId.get(gem.skill_id)?.is_support === false ? 'active' : undefined}
                  data-colour={byId.get(gem.skill_id)?.colour ?? undefined}>
                  <AppTooltip content={gem.skill_id === '' ? tt('skills.unresolvedHint') : undefined}><span className="gem-name" tabIndex={gem.skill_id === '' ? 0 : undefined}>
                    {gemLabel(gem)}
                    {byId.has(gem.skill_id) && <small className="gem-kind-label">{tt(byId.get(gem.skill_id)!.is_support ? 'compare.support' : 'compare.active')}</small>}
                    {byId.get(gem.skill_id)?.is_lineage && <span className="gem-lineage-badge">{tt('picker.lineage')}</span>}
                  </span></AppTooltip>
                  <span className="gem-controls">
                    <label className="gem-enabled"><AppInput type="checkbox" aria-label={tt('skills.gemEnabled')}
                      checked={gem.enabled !== false} disabled={session.busy}
                      onChange={event => updateGroup(idx, withGemEnabled(group, gemIdx, event.target.checked, byId))} />{tt('skills.gemEnabled')}</label>
                    <label className="gem-count">{tt('skills.count')}<AppInput type="number" min={0} step="any" value={gem.count ?? 1}
                      aria-label={tt('skills.gemCount')} disabled={session.busy} onChange={event => {
                        const count = Number(event.target.value);
                        if (Number.isFinite(count) && count >= 0) updateGroup(idx, { gems: group.gems.map((item, index) => index === gemIdx ? { ...item, count } : item) });
                      }} /></label>
                    <label className="gem-number">{tt('skills.level')}<AppInput
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
                    <label className="gem-number">{tt('skills.quality')}<AppInput
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
                    <AppTooltip content={tt('skills.corruptedHint')}><label className="gem-corrupted">
                      {tt('skills.corrupted')}
                      <AppInput
                        type="checkbox"
                        checked={gem.corrupted === true}
                        disabled={session.busy}
                        aria-label={tt('skills.corrupted')}
                        onChange={(e) => updateGroup(idx, {
                          gems: group.gems.map((g, i) => (i === gemIdx ? withGemCorrupted(g, e.target.checked) : g)),
                        })}
                      />
                    </label></AppTooltip>
                    <AppButton
                      className="skill-remove"
                      disabled={session.busy || catalog.length === 0}
                      title={tt('skills.removeGem')}
                      onClick={() => updateGroup(idx, removeSkillGem(group, gemIdx, byId))}
                    >
                      ×
                    </AppButton>
                  </span>
                </li>
              ))}
            </ul>
          );
          const minionControls = group.gems.map((gem, gemIndex) => <MinionSelection key={gemIndex} session={session} lang={lang}
            groupIndex={idx} gemIndex={gemIndex} gemName={gemLabel(gem)} />);
          return (
            <div
              key={idx}
              className={`skill-group${isMain ? ' is-main' : ''}${group.enabled ? '' : ' is-disabled'}${isOpen ? ' is-selected' : ''}`}
            >
              <div className="skill-group-header">
                <AppButton
                  className={`skill-main-toggle${isMain ? ' is-on' : ''}`}
                  aria-pressed={isMain}
                  disabled={session.busy}
                  title={tt('skills.setMain')}
                  onClick={() => session.updateParams({ main_socket_group: idx })}
                >
                  ★
                </AppButton>
                <AppButton
                  className="skill-group-title"
                  aria-expanded={isOpen}
                  onClick={() => setOpenIdx(idx)}
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
                  {supportGems.length > 0 && (
                    <span className="skill-group-supports">
                      {supportGems.map((g) => `${gemLabel(g)}${byId.get(g.skill_id)?.is_lineage ? ` [${tt('picker.lineage')}]` : ''}`).join(' · ')}
                    </span>
                  )}
                </AppButton>
                <span className="skill-dps-value" data-kind={dps?.kind ?? 'unavailable'}>
                  {dps?.kind === 'damage' ? <>{formatStatValue(dps.dps, 'int')}<span className="sr-only"> DPS</span></>
                    : dps?.kind === 'none' ? <>—<span className="sr-only">{w('deals no damage', '不造成伤害', '不造成傷害')}</span></> : null}
                </span>
                <span className="skill-dps-bar" aria-hidden="true">
                  {dps?.kind === 'damage' && <span style={{ width: `${barShare(dps.dps, dpsTop) * 100}%` }} />}
                </span>
                <label className="skill-group-toggle">
                  <AppInput
                    type="checkbox"
                    checked={group.enabled}
                    disabled={session.busy}
                    onChange={(e) => updateGroup(idx, { enabled: e.target.checked })}
                  />
                  {tt('skills.enabled')}
                </label>
                <AppButton
                  className="skill-remove"
                  disabled={session.busy}
                  title={tt('skills.removeGroup')}
                  onClick={() => {
                    session.removeSocketGroup(idx);
                    setOpenIdx(null);
                  }}
                >
                  ×
                </AppButton>
              </div>
              {isOpen && (
                <div className="skill-detail">
                  <header className="skill-detail-head">
                    <h3 className="skill-detail-title">{selectedId ? gemName(selectedId) : tt('skills.emptyGroup')}</h3>
                    <div className="skill-detail-tags">
                      {isMain && <span className="skill-detail-tag">{tt('skills.main')}</span>}
                      {grantedSourceLabel(lang, group.source) && <span className="skill-detail-tag">{grantedSourceLabel(lang, group.source)}</span>}
                    </div>
                    {dps?.kind === 'damage' && <div className="skill-detail-dps"><span>{tt('sidebar.combinedDps')}</span><strong>{formatStatValue(dps.dps, 'int')}</strong></div>}
                    {active && <div className="skill-detail-controls">
                      {weaponBinding}
                      {effectControl}
                    </div>}
                  </header>
                  {active && <SkillFormControls session={session} lang={lang} groupIndex={idx} catalog={byId} />}
                  {active && <section className="skill-detail-section" aria-label={w('Gems in this group', '本组宝石', '本組寶石')}>
                    <h4 className="skill-section-label">{w('Gems', '宝石', '寶石')}</h4>
                    <div className="gem-column-labels" aria-hidden="true">
                      <span /><span>{tt('calcs.skill')}</span><span>{tt('skills.level')}</span><span>{tt('skills.quality')}</span>
                      <span>{tt('skills.count')}</span><span>{tt('skills.corrupted')}</span><span />
                    </div>
                    {gemList}
                  </section>}
                  <section className="skill-detail-section" aria-label={w('Add a support gem', '添加辅助宝石', '新增輔助寶石')}>
                    <h4 className="skill-section-label">{w('Add a support', '添加辅助', '新增輔助')}</h4>
                    {optimizer}
                    {supportPicker}
                  </section>
                  {active && <details className="skill-advanced">
                    <summary>{w('Advanced', '高级', '進階')}</summary>
                    <div className="skill-advanced-body">
                      {fullDps}
                      {minionControls}
                      {note}
                    </div>
                  </details>}
                </div>
              )}
            </div>
          );
        })}
        {groups.length > 1 && <AppButton className="skills-list-toggle" aria-expanded={showAllGroups} onClick={() => setShowAllGroups(value => !value)}>
          {showAllGroups ? w('Show selected skill only', '收起其他技能', '收起其他技能') : w(`All skill groups (${groups.length})`, `全部技能组（${groups.length}）`, `全部技能組（${groups.length}）`)}
        </AppButton>}
      </div>
      {active && <details className="skills-advanced" open={session.library.skillSets.length > 0 || undefined}>
        <summary>{tt('sets.title')}</summary>
        <SkillSets session={session} lang={lang} />
      </details>}
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
          {localizedName(lang, set.name, session.classNames.ascendancies[set.name] ?? session.classNames.classes[set.name])}（{set.groups.length}）
          <AppButton disabled={session.busy} onClick={() => session.applySkillSet(set.id)}>
            {tt('sets.apply')}
          </AppButton>
          <AppButton className="skill-remove" onClick={() => session.removeSkillSet(set.id)}>
            ×
          </AppButton>
        </span>
      ))}
      <AppInput
        placeholder={tt('sets.namePlaceholder')}
        value={name}
        onChange={(e) => setName(e.target.value)}
        aria-label={tt('sets.namePlaceholder')}
      />
      <AppButton
        disabled={!name.trim() || session.socketGroups.length === 0}
        onClick={() => {
          session.saveSkillSet(name.trim());
          setName('');
        }}
      >
        {tt('sets.save')}
      </AppButton>
    </div>
  );
}
