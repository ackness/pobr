import { encounterTierLabel } from '../../lib/configEncounter';
import { buildDisplayName, stageDisplayName } from '../../lib/workspaceNames';
import { AppButton, AppInput, AppSelectControl } from '../shared/controls';
import { Tabs as TabsPrimitive } from 'radix-ui';
import { useEffect, useMemo, useState } from 'react';
import type { BuildSession } from '../../hooks/useBuildSession';
import { toRequest } from '../../hooks/useBuildSession';
import { useSkillCatalog } from '../../hooks/useSkillName';
import { formatApiError } from '../../api/error';
import type { SocketGroupInput } from '../../api/types';
import { compareBuilds, comparisonGemForm, hasComparisonSource, isPercentageStat, defaultCompareSides, configDifferences, copySection, formatComparisonDelta, itemDifferences, resolvedTreeVersion, skillDifferences, weaponDifferences, type ConfigBasis, type CopySection } from '../../lib/compare';
import { bindT, slotLabel, statNameLabel, t, type Lang } from '../../lib/i18n';
import { activeSkillEffects } from '../../lib/skillSelection';
import { skillDisplayName } from '../../lib/skillNames';
import { formatNumber, STAT_SECTIONS, formatStatValue } from '../../lib/statDisplay';
import { UnincludedModifiers } from '../shared/UnincludedModifiers';
import { PageHeader } from '../shared/PageHeader';
import { Button } from '../ui/button';
import { words } from '../../app/text';
import { useNavigateToPage } from '../../app/pages/navigate';
import { COMPARE_TABS, defaultCompareTab, deltaTone, headlineDeltas, signedPercent, summaryLine, treeDifferenceCount, type CompareTab, type DiffCounts } from '../../app/pages/compare/summary';
import './compare.css';
import { TreeComparison } from './TreeComparison';
import { ItemDisplay } from '../items/ItemDisplay';
import { ConfigComparison } from './ConfigComparison';
import { classDisplayName } from '../../lib/nativeDisplayNames';
import { baselineTime } from '../../lib/comparisonBaseline';
import { headlineDpsStat, mainMinion, minionDpsNote } from '../../lib/mainMinion';
import { useLocalizedNames } from '../../hooks/useLocalizedLines';

export function ComparePanel({ session, lang }: { session: BuildSession; lang: Lang }) {
  const goTo = useNavigateToPage();
  const tt = bindT(lang);
  const catalog = useSkillCatalog();
  const entries = useMemo(() => session.workspace?.builds.flatMap((build, buildIndex) => build.stages.map((stage, stageIndex) => ({
    buildId: build.id, stageId: stage.id, saved: stage.saved, drafts: stage.drafts,
    label: `${buildDisplayName(build, buildIndex, lang)} / ${stageDisplayName(stage, stageIndex, lang)}`,
  }))) ?? [], [session.workspace, lang]);
  const build = session.workspace?.builds.find(build => build.id === session.workspace?.activeBuild);
  const active = build?.activeStage;
  const baseline = build?.baseline ? {
    buildId: build.id, stageId: `baseline:${build.id}`, saved: build.baseline.saved, drafts: {},
    label: tt('compare.baselineOption').replace('{time}', baselineTime(build.baseline.recordedAt, lang)),
  } : undefined;
  const options = baseline ? [baseline, ...entries] : entries;
  const defaults = defaultCompareSides(entries, active, baseline);
  const [aId, setAId] = useState(defaults.a?.stageId);
  const [bId, setBId] = useState(defaults.b?.stageId);
  const [basis, setBasis] = useState<ConfigBasis>('own');
  const [changedOnly, setChangedOnly] = useState(true);
  const [message, setMessage] = useState('');
  const [tab, setTab] = useState<CompareTab | null>(null);
  const a = options.find(entry => entry.stageId === aId) ?? defaults.a;
  const b = options.find(entry => entry.stageId === bId);
  const key = JSON.stringify([a?.saved.state, b?.saved.state, basis]);
  const [result, setResult] = useState<{ key: string; value?: Awaited<ReturnType<typeof compareBuilds>>; error?: string } | null>(null);
  useEffect(() => {
    if (!a || !b) return;
    let cancelled = false;
    compareBuilds(toRequest(a.saved.state), toRequest(b.saved.state), basis)
      .then(value => { if (!cancelled) setResult({ key, value }); })
      .catch(error => { if (!cancelled) setResult({ key, error: formatApiError(error) }); });
    return () => { cancelled = true; };
  }, [key]);
  const current = result?.key === key ? result : null;
  const loading = !current;
  const value = current?.value;
  const drafts = !!a && Object.keys(a.drafts).length > 0;
  const canApply = !!value && !loading && !session.busy && a !== b && !drafts && a !== baseline && b !== baseline;
  const apply = (section: CopySection) => {
    if (!a || !b || !canApply || !hasComparisonSource(b.saved.state, section)) return;
    const next = copySection(a.saved.state, b.saved.state, section);
    setMessage(session.applyComparedStage(a, b, next) ? tt('compare.applied') : tt('compare.stale'));
  };
  const applyButton = (section: CopySection, append = false) => {
    if (baseline && (a === baseline || b === baseline)) return null;
    const hasSource = !!b && hasComparisonSource(b.saved.state, section);
    return <AppButton type="button" disabled={!canApply || !hasSource}
      onClick={() => apply(section)}>{tt(!hasSource ? 'compare.emptySource' : append ? 'compare.append' : 'compare.apply')}</AppButton>;
  };
  const sections = [{ title: { 'en-US': t('en-US', 'sidebar.mainSkill'), 'zh-TW': t('zh-TW', 'sidebar.mainSkill'), 'zh-CN': t('zh-CN', 'sidebar.mainSkill') },
    rows: ([['MainMinionDPS', 'sidebar.minionHitDps'], ['MainSkillHitDPS', 'sidebar.hitDps'], ['MainSkillDotDPS', 'sidebar.dotDps'], ['MainSkillCombinedDPS', 'sidebar.combinedDps']] as const)
      .map(([id, label]) => ({ id, label: { 'en-US': t('en-US', label), 'zh-TW': t('zh-TW', label), 'zh-CN': t('zh-CN', label) }, format: 'float2' as const })),
  }, ...STAT_SECTIONS];
  const absent = tt('compare.notSet');
  const skill = (group: SocketGroupInput | undefined, main: boolean, other: SocketGroupInput | undefined) => {
    if (!group) return <p>{absent}</p>;
    const effect = activeSkillEffects(group, catalog)[(group.main_active_skill ?? 1) - 1];
    return <>
      <p>{main && <strong>{tt('sidebar.mainSkill')} · </strong>}{tt(group.enabled ? 'compare.enabled' : 'compare.disabled')}
        {group.weapon_set && <> · {tt('compare.weaponSet')} {group.weapon_set}</>}</p>
      <ul>{group.gems.map((gem, index) => {
        const otherGem = other?.gems[index]?.skill_id === gem.skill_id ? other.gems[index] : other?.gems.find(value => value.skill_id === gem.skill_id);
        const form = comparisonGemForm(gem, otherGem, catalog, lang, effect?.gemIndex === index ? effect.skillId : gem.skill_id);
        return <li key={index}>
          <strong>{gem.skill_id ? skillDisplayName(gem.skill_id, lang, catalog, gem.name_spec ?? undefined) : gem.name_spec ? `${gem.name_spec} · ${tt('skills.unresolved')}` : tt('skills.unresolved')}</strong> · {tt(catalog.get(gem.skill_id)?.is_support ? 'compare.support' : 'compare.active')}
          <span> · {tt('compare.level')} {gem.level} · {tt('compare.quality')} {gem.quality}%</span>
          {form && <span> · {form}</span>}
          {(gem.enable_global_1 === false || gem.enable_global_2 === false) && <span> · {tt('compare.disabled')}</span>}
        </li>;
      })}</ul>
      <details><summary className="disclosure" data-disclosure-level="inline">{tt('compare.advanced')}</summary><pre>{JSON.stringify(group, null, 2)}</pre></details>
    </>;
  };
  const textRow = (id: string, title: string, before: string | undefined, after: string | undefined, section: CopySection) => {
    return <article className="compare-difference" key={id} data-item={id}>
      <header><h4>{title}</h4>{applyButton(section)}</header>
      <div className="compare-pair">{[before, after].map((text, index) => <div key={index}>
        <strong>{tt(index === 0 ? 'compare.before' : 'compare.after')}</strong>
        {text ? <ItemDisplay rawDetails text={text} lang={lang} /> : <p>{absent}</p>}
      </div>)}</div>
    </article>;
  };
  const stageWord = (side: 'A' | 'B') => tt(side === 'A' ? 'compare.before' : 'compare.after');
  const identity = (entry: NonNullable<typeof a>) => [`${tt('compare.level')} ${entry.saved.state.character.level}`,
    classDisplayName(session.classNames ?? { classes: {}, ascendancies: {} }, entry.saved.state.character.class_name, lang, 'classes'),
    classDisplayName(session.classNames ?? { classes: {}, ascendancies: {} }, entry.saved.state.character.ascendancy_name, lang, 'ascendancies'),
    `${tt('compare.weaponSet')} ${entry.saved.state.weaponSwap?.active ?? 1}`].filter(Boolean).join(' · ');
  const pick = (side: 'A' | 'B', id: string) => {
    if (side === 'A') setAId(id);
    else setBId(id);
    setTab(null); setMessage('');
  };
  const stagePickers = <div className="cmp-stages">
    {([['A', a], ['B', b]] as const).map(([side, entry]) => <div className="cmp-stage" key={side}>
      <span className="cmp-stage-name"><i className="cmp-mark" data-side={side.toLowerCase()} aria-hidden="true" />{side}</span>
      <AppSelectControl aria-label={stageWord(side)} value={entry?.stageId ?? ''} onChange={event => pick(side, event.target.value)}>
        {side === 'B' && <option value="">{tt('compare.chooseB')}</option>}
        {options.map(option => <option key={option.stageId} value={option.stageId}>{option.label}{baseline && option.stageId === active ? ` · ${tt('compare.current')}` : ''}</option>)}
      </AppSelectControl>
    </div>)}
    <Button variant="ghost" size="sm" aria-label={tt('compare.swap')} disabled={!a || !b} onClick={() => { setAId(b?.stageId); setBId(a?.stageId); setMessage(''); }}>⇄</Button>
  </div>;
  const statDefs = new Map(sections.flatMap(section => section.rows.map(def => [def.id, def] as const)));
  const itemChanges = a && b ? itemDifferences(a.saved.state, b.saved.state) : [];
  const weaponChanges = a && b ? weaponDifferences(a.saved.state, b.saved.state) : [];
  const skillChanges = a && b ? skillDifferences(a.saved.state, b.saved.state) : [];
  const configChanges = a && b ? [
    ...configDifferences(a.saved.state, b.saved.state).map(row => ({ ...row, action: { kind: 'config', key: row.key } as CopySection })),
    { key: tt('compare.enemy'), before: encounterTierLabel(lang, a.saved.state.params.enemy_tier, a.saved.state.params.config_inputs), after: encounterTierLabel(lang, b.saved.state.params.enemy_tier, b.saved.state.params.config_inputs), action: { kind: 'enemy' } as CopySection },
    { key: tt('compare.mods'), before: a.saved.state.params.custom_modifier_blocks ?? a.saved.state.params.extra_modifiers,
      after: b.saved.state.params.custom_modifier_blocks ?? b.saved.state.params.extra_modifiers, action: { kind: 'modifiers' } as CopySection },
  ].filter(row => JSON.stringify(row.before) !== JSON.stringify(row.after)) : [];
  const sameTree = !!a && !!b && resolvedTreeVersion(a.saved.state.treeVersion, session.dataVersion) === resolvedTreeVersion(b.saved.state.treeVersion, session.dataVersion);
  const treeChanges = a && b ? treeDifferenceCount(a.saved.state, b.saved.state, sameTree) : 0;
  const counts: DiffCounts = {
    stats: value ? [...statDefs.keys()].filter(id => value.stats.find(row => row.id === id)?.changed).length : 0,
    items: itemChanges.length + weaponChanges.length, skills: skillChanges.length, config: configChanges.length, tree: treeChanges ?? 0,
  };
  const activeTab: CompareTab = tab ?? (value ? defaultCompareTab(counts) : 'stats');
  const tabLabel: Record<CompareTab, string> = { stats: words(lang, 'Stats', '属性', '屬性'), items: tt('tab.items'), skills: tt('tab.skills'), config: tt('tab.config'), tree: tt('tab.tree') };
  const minionOutput = mainMinion(value?.after) ? value?.after : value?.before;
  const headlines = value ? headlineDeltas(value.stats, headlineDpsStat(minionOutput)) : [];
  const minion = mainMinion(minionOutput);
  const [minionName] = useLocalizedNames(minion ? [{ kind: 'monster', id: minion.monster_variety_id, name: minion.name }] : [], lang);
  return <section className="compare-panel" aria-labelledby="compare-heading">
    <PageHeader id="compare-heading" title={tt('compare.title')} description={tt('compare.hint')} />
    <TabsPrimitive.Root className="cmp-root" value={activeTab} onValueChange={next => setTab(next as CompareTab)}>
      <div className="cmp-bar">
        {stagePickers}
      </div>
      <div className="cmp-context">
        <div className="cmp-identities">{([['A', a], ['B', b]] as const).map(([side, entry]) => entry && <small key={side}>{side} · {identity(entry)}</small>)}</div>
        <label className="cmp-config-basis">{tt('compare.basis')}
          <AppSelectControl aria-label={tt('compare.basis')} value={basis} onChange={event => setBasis(event.target.value as ConfigBasis)}>
            <option value="own">{tt('compare.own')}</option><option value="a">{tt('compare.useA')}</option>
          </AppSelectControl>
        </label>
        <p className="cmp-note">{tt('compare.direction')}</p>
        {baseline && <p className="cmp-note">{tt('compare.baselineHint')}</p>}
        {a && b && <div className="cmp-summary" role="status">
          {loading ? <span>{tt('compare.calculating')}</span> : <>
            {headlines.map(line => <span className="cmp-headline" key={line.id} data-tone={line.tone}>{statDefs.get(line.id)?.label[lang] ?? statNameLabel(lang, line.id)} <b>{signedPercent(line.percent)}</b></span>)}
            <span className="cmp-sentence">{summaryLine(lang, counts)}{treeChanges === null && ` ${tt('compare.treeMismatch')}`}</span></>}
        </div>}
        {minion && <p className="cmp-note">{minionDpsNote(lang, minionName ?? minion.name, minionOutput?.stats.find(stat => stat.id === 'ActiveMinionLimit')?.value)}</p>}
        {a && b && <>
          <TabsPrimitive.List className="cmp-tablist segmented" aria-label={words(lang, 'Comparison sections', '对比分区', '比較分區')}>
            {COMPARE_TABS.map(id => <TabsPrimitive.Trigger key={id} value={id} className="cmp-tab">
              {tabLabel[id]}<span className="cmp-count" data-zero={counts[id] === 0 || (id === 'stats' && !value) || undefined}>{id === 'stats' && !value ? '…' : id === 'tree' && treeChanges === null ? '≠' : counts[id]}</span>
            </TabsPrimitive.Trigger>)}
          </TabsPrimitive.List>
        </>}
      </div>
      {a && b ? <>
        {(Object.keys(a.drafts).length > 0 || Object.keys(b.drafts).length > 0) && <p className="cmp-note">{tt('compare.draftHint')}</p>}
        {drafts && <p role="note" className="cmp-note cmp-warn">{tt('compare.drafts')}</p>}
        {message && <p role="status" className="cmp-note cmp-message">{message}</p>}
        {current?.error && <p role="alert" className="cmp-note cmp-warn">{current.error}</p>}

        <TabsPrimitive.Content value="stats" className="cmp-panel">
          {value && <article className="compare-summary cmp-stats">
            <h3 className="sr-only">{tt('compare.summary')}</h3>
            <div className="cmp-toolbar">
              <label className="cmp-toggle"><AppInput type="checkbox" checked={changedOnly} onChange={event => setChangedOnly(event.target.checked)} /> {tt('compare.changed')}</label>
              <p className="cmp-note">{tt('compare.percentHint')}</p>
            </div>
            {!(changedOnly && counts.stats === 0) && <div className="compare-table-wrap"><table><thead><tr><th scope="col">{tt('compare.summary')}</th><th scope="col">{tt('compare.before')}</th><th scope="col">{tt('compare.after')}</th><th scope="col">{tt('compare.delta')}</th><th scope="col">{tt('compare.percent')}</th></tr></thead>
            {sections.map(section => {
              const rows = section.rows.map(def => ({ def, row: value.stats.find(row => row.id === def.id) }))
                .filter(({ row }) => row && (!changedOnly || row.changed));
              if (!rows.length) return null;
              return <tbody key={section.title[lang]}><tr className="cmp-section-row"><th scope="rowgroup" colSpan={5}>{section.title[lang]}</th></tr>{rows.map(({ def, row: maybe }) => { const row = maybe!; const tone = deltaTone(row.id, row.delta); return <tr key={row.id} data-stat={row.id}>
                  <th scope="row">{def.label[lang]}</th>{[row.before, row.after].map((number, i) => <td key={i} data-label={i === 0 ? 'A' : 'B'}>{typeof number === 'number' && Number.isFinite(number) ? formatStatValue(number, def.format, lang) : number === Infinity ? '∞' : number === -Infinity ? '−∞' : <span role="img" aria-label={tt('compare.unavailable')}>—</span>}</td>)}
                  <td className="cmp-delta" data-tone={tone} data-label={tt(isPercentageStat(def.format) ? 'compare.points' : 'compare.delta')} aria-label={isPercentageStat(def.format) ? `${tt('compare.points')}${lang === 'en-US' ? ': ' : '：'}${formatComparisonDelta(row.delta, def.format, lang)}` : undefined}>{formatComparisonDelta(row.delta, def.format, lang)}</td>
                  <td className="cmp-delta" data-tone={tone} data-label={tt('compare.percent')}>{row.percent === null ? '—' : `${row.percent > 0 ? '+' : ''}${formatNumber(row.percent, 1, 1)}%`}</td>
                </tr>; })}</tbody>;
            })}</table></div>}
            {changedOnly && counts.stats === 0 && <p className="cmp-none">{tt('compare.none')}</p>}
            <p className="cmp-note">{session.dataVersion ? tt('compare.basisHint').replace('{value}', session.dataVersion) : tt('compare.basisHint').replace(/\s*[(（]\{value\}[)）]/, '')}</p>
            <details className="cmp-details" open={[value.before, value.after].some(output => output.unsupported_modifiers.length || output.item_errors.length)}><summary className="disclosure" data-disclosure-level="group">{tt('compare.diagnostics')}</summary>{[value.before, value.after].map((output, index) => <div key={index}>
              <strong>{index === 0 ? 'A' : 'B'} · {tt('sidebar.mainSkill')}{lang === 'en-US' ? ': ' : '：'}{output.main_skill ? skillDisplayName(output.main_skill.skill_id, lang, catalog) : absent}</strong>
              <UnincludedModifiers lines={output.unsupported_modifiers} lang={lang} /><ul>{output.item_errors.map((error, i) => <li key={i}>{slotLabel(lang, error.slot)}{lang === 'en-US' ? ': ' : '：'}{error.message}</li>)}</ul>
            </div>)}</details>
          </article>}
        </TabsPrimitive.Content>

        <TabsPrimitive.Content value="items" className="cmp-panel">
          <h3 className="sr-only">{tt('tab.items')}</h3>
          {itemChanges.map(row => textRow(`${row.section}:${row.slot}`, slotLabel(lang, row.slot), row.before, row.after, { kind: 'item', section: row.section, slot: row.slot }))}
          {weaponChanges.map(row => textRow(`weapon:${row.set}:${row.slot}`, `${tt('compare.weaponSet')} ${row.set} · ${slotLabel(lang, row.slot)}`, row.before, row.after, { kind: 'weapon', set: row.set, slot: row.slot }))}
          {!counts.items && <p className="cmp-none">{tt('compare.none')}</p>}
        </TabsPrimitive.Content>

        <TabsPrimitive.Content value="skills" className="cmp-panel">
          <h3 className="sr-only">{tt('tab.skills')}</h3>
          {skillChanges.map(row => <article className="compare-difference" key={row.index}>
            <header><h4>{tt('compare.group')} {row.index + 1}</h4>{row.before?.source || row.after?.source ? <p>{tt('compare.granted')}</p> : (JSON.stringify(row.before) !== JSON.stringify(row.after) || row.mainB) && applyButton({ kind: 'skill', index: row.index }, row.index >= a.saved.state.socketGroups.length)}</header>
            <div className="compare-pair"><div><strong>A</strong>{skill(row.before, row.mainA, row.after)}</div><div><strong>B</strong>{skill(row.after, row.mainB, row.before)}</div></div>
          </article>)}
          {!counts.skills && <p className="cmp-none">{tt('compare.none')}</p>}
        </TabsPrimitive.Content>

        <TabsPrimitive.Content value="config" className="cmp-panel">
          <h3 className="sr-only">{tt('compare.config')}</h3>
          <ConfigComparison rows={configChanges} lang={lang} applyButton={applyButton} />
          {!counts.config && <p className="cmp-none">{tt('compare.none')}</p>}
        </TabsPrimitive.Content>

        <TabsPrimitive.Content value="tree" className="cmp-panel">
          <TreeComparison a={a.saved.state} b={b.saved.state} before={value?.before} after={value?.after}
            dataVersion={session.dataVersion} meta={session.treeMeta} lang={lang} />
        </TabsPrimitive.Content>
      </> : <div className="cmp-empty"><p>{tt(defaults.b ? 'compare.chooseB' : 'compare.noBaseline')}</p><p>{tt('compare.needStages')}</p>
        <div className="cmp-empty-actions">
          <Button onClick={session.saveComparisonBaseline} disabled={session.busy}>{tt('compare.saveBaseline')}</Button>
          <Button variant="outline" onClick={() => goTo('build')}>{words(lang, 'Open Build library', '前往 BD 库', '前往 BD 庫')}</Button>
        </div></div>}
    </TabsPrimitive.Root>
  </section>;
}
