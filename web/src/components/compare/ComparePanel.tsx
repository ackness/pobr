import { AppButton, AppInput, AppSelectControl } from '../shared/controls';
import { Tabs as TabsPrimitive } from 'radix-ui';
import { useEffect, useMemo, useState } from 'react';
import type { BuildSession } from '../../hooks/useBuildSession';
import { toRequest } from '../../hooks/useBuildSession';
import { useSkillCatalog } from '../../hooks/useSkillName';
import { formatApiError } from '../../api/error';
import type { SocketGroupInput } from '../../api/types';
import { compareBuilds, configDifferences, copySection, diffText, formatComparisonDelta, itemDifferences, resolvedTreeVersion, skillDifferences, weaponDifferences, type ConfigBasis, type CopySection } from '../../lib/compare';
import { bindT, statNameLabel, t, type Lang } from '../../lib/i18n';
import { skillDisplayName } from '../../lib/skillNames';
import { STAT_SECTIONS, formatStatValue } from '../../lib/statDisplay';
import { PageHeader } from '../shared/PageHeader';
import { Button } from '../ui/button';
import { words } from '../../app/text';
import { useNavigateToPage } from '../../app/pages/navigate';
import { COMPARE_TABS, collapseContext, defaultCompareTab, deltaTone, headlineDeltas, signedPercent, summaryLine, treeDifferenceCount, type CompareTab, type DiffCounts } from '../../app/pages/compare/summary';
import './compare.css';
import { TreeComparison } from './TreeComparison';

export function ComparePanel({ session, lang }: { session: BuildSession; lang: Lang }) {
  const goTo = useNavigateToPage();
  const tt = bindT(lang);
  const catalog = useSkillCatalog();
  const entries = useMemo(() => session.workspace?.builds.flatMap(build => build.stages.map(stage => ({
    buildId: build.id, stageId: stage.id, saved: stage.saved, drafts: stage.drafts,
    label: `${build.name || words(lang, 'Build', 'BD')} / ${stage.name || tt('compare.stage')}`,
  }))) ?? [], [session.workspace, lang]);
  const active = session.workspace?.builds.find(build => build.id === session.workspace?.activeBuild)?.activeStage;
  const [aId, setAId] = useState(active);
  const [bId, setBId] = useState(entries.find(entry => entry.stageId !== active)?.stageId);
  const [basis, setBasis] = useState<ConfigBasis>('own');
  const [changedOnly, setChangedOnly] = useState(true);
  const [message, setMessage] = useState('');
  const [tab, setTab] = useState<CompareTab | null>(null);
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const a = entries.find(entry => entry.stageId === aId) ?? entries[0];
  const b = entries.find(entry => entry.stageId === bId) ?? entries.find(entry => entry !== a) ?? a;
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
  const canApply = !loading && !session.busy && a !== b && !drafts;
  const apply = (section: CopySection) => {
    if (!a || !b || !canApply) return;
    const next = copySection(a.saved.state, b.saved.state, section);
    setMessage(session.applyComparedStage(a, b, next) ? tt('compare.applied') : tt('compare.stale'));
  };
  const applyButton = (section: CopySection, append = false) => <AppButton type="button" disabled={!canApply}
    onClick={() => apply(section)}>{tt(append ? 'compare.append' : 'compare.apply')}</AppButton>;
  const sections = [{ title: { 'en-US': t('en-US', 'sidebar.mainSkill'), 'zh-TW': t('zh-TW', 'sidebar.mainSkill'), 'zh-CN': t('zh-CN', 'sidebar.mainSkill') },
    rows: ([['MainSkillHitDPS', 'sidebar.hitDps'], ['MainSkillDotDPS', 'sidebar.dotDps'], ['MainSkillCombinedDPS', 'sidebar.combinedDps']] as const)
      .map(([id, label]) => ({ id, label: { 'en-US': t('en-US', label), 'zh-TW': t('zh-TW', label), 'zh-CN': t('zh-CN', label) }, format: 'float2' as const })),
  }, ...STAT_SECTIONS];
  const absent = tt('compare.notSet');
  const showValue = (value: unknown) => value === undefined ? absent : typeof value === 'string' ? value : JSON.stringify(value, null, 2);
  const skill = (group: SocketGroupInput | undefined, main: boolean) => !group ? <p>{absent}</p> : <>
    <p>{main && <strong>{tt('sidebar.mainSkill')} · </strong>}{tt(group.enabled ? 'compare.enabled' : 'compare.disabled')}
      {group.weapon_set && <> · {tt('compare.weaponSet')} {group.weapon_set}</>}</p>
    <ul>{group.gems.map((gem, index) => <li key={index}>
      <strong>{skillDisplayName(gem.skill_id, lang, catalog)}</strong> · {tt(catalog.get(gem.skill_id)?.is_support ? 'compare.support' : 'compare.active')}
      <span> · {tt('compare.level')} {gem.level} · {tt('compare.quality')} {gem.quality}%</span>
      <span> · {tt('compare.form')} {gem.stat_set_index ?? 1}</span>
      {(gem.enable_global_1 === false || gem.enable_global_2 === false) && <span> · {tt('compare.disabled')}</span>}
    </li>)}</ul>
    <details><summary>{tt('compare.rawGroup')}</summary><pre>{JSON.stringify(group, null, 2)}</pre></details>
  </>;
  const textRow = (id: string, title: string, before: string | undefined, after: string | undefined, section: CopySection) => {
    const lines = diffText(before, after);
    const rows = (['before', 'after'] as const).map(side => collapseContext(lines[side], expanded[id] ? Infinity : 1));
    return <article className="compare-difference" key={id} data-item={id}>
      <header><h4>{title}</h4><span className="cmp-diff-actions">
        {(expanded[id] || rows.some(side => side.some(row => row.kind === 'gap'))) && <Button variant="ghost" size="sm" aria-pressed={!!expanded[id]}
          onClick={() => setExpanded(current => ({ ...current, [id]: !current[id] }))}>{expanded[id] ? words(lang, 'Changes only', '只看改动', '只看改動') : words(lang, 'Show all lines', '显示全部行', '顯示全部行')}</Button>}
        {applyButton(section)}</span></header>
      <div className="compare-pair">{(['before', 'after'] as const).map((side, index) => <div key={side}><strong>{index === 0 ? 'A' : 'B'}</strong>
        <pre>{rows[index].length ? rows[index].map((row, i) => row.kind === 'gap'
          ? <span key={i} className="cmp-gap">{words(lang, `⋯ ${row.count} unchanged ${row.count === 1 ? 'line' : 'lines'}`, `⋯ ${row.count} 行未改动`, `⋯ ${row.count} 行未改動`)}{'\n'}</span>
          : <span key={i} className={row.changed ? `compare-line-${side}` : ''}>{row.changed ? index ? '+ ' : '− ' : '  '}{row.text}{'\n'}</span>) : absent}</pre>
      </div>)}</div>
    </article>;
  };
  const stageWord = (side: 'A' | 'B') => words(lang, `Build / stage ${side}`, `BD / 阶段 ${side}`, `BD / 階段 ${side}`);
  if (entries.length < 2 || !a || !b) return <section className="compare-panel" aria-labelledby="compare-heading">
    <PageHeader id="compare-heading" title={tt('compare.title')} description={tt('compare.hint')} />
    <div className="cmp-empty"><p>{tt('compare.needStages')}</p>
      <Button onClick={() => goTo('build')}>{words(lang, 'Open the BD library', '前往 BD 库', '前往 BD 庫')}</Button></div>
  </section>;
  const statDefs = new Map(sections.flatMap(section => section.rows.map(def => [def.id, def] as const)));
  const itemChanges = itemDifferences(a.saved.state, b.saved.state);
  const weaponChanges = weaponDifferences(a.saved.state, b.saved.state);
  const skillChanges = skillDifferences(a.saved.state, b.saved.state);
  const configChanges = [
    ...configDifferences(a.saved.state, b.saved.state).map(row => ({ ...row, action: { kind: 'config', key: row.key } as CopySection })),
    { key: tt('compare.enemy'), before: a.saved.state.params.enemy_tier, after: b.saved.state.params.enemy_tier, action: { kind: 'enemy' } as CopySection },
    { key: tt('compare.mods'), before: a.saved.state.params.custom_modifier_blocks ?? a.saved.state.params.extra_modifiers,
      after: b.saved.state.params.custom_modifier_blocks ?? b.saved.state.params.extra_modifiers, action: { kind: 'modifiers' } as CopySection },
  ].filter(row => JSON.stringify(row.before) !== JSON.stringify(row.after));
  const sameTree = resolvedTreeVersion(a.saved.state.treeVersion, session.dataVersion) === resolvedTreeVersion(b.saved.state.treeVersion, session.dataVersion);
  const treeChanges = treeDifferenceCount(a.saved.state, b.saved.state, sameTree);
  const counts: DiffCounts = {
    stats: value ? [...statDefs.keys()].filter(id => value.stats.find(row => row.id === id)?.changed).length : 0,
    items: itemChanges.length + weaponChanges.length, skills: skillChanges.length, config: configChanges.length, tree: treeChanges ?? 0,
  };
  const activeTab: CompareTab = tab ?? (value ? defaultCompareTab(counts) : 'stats');
  const tabLabel: Record<CompareTab, string> = { stats: words(lang, 'Stats', '属性', '屬性'), items: tt('tab.items'), skills: tt('tab.skills'), config: tt('tab.config'), tree: tt('tab.tree') };
  const headlines = value ? headlineDeltas(value.stats) : [];
  const identity = (entry: typeof a) => [`${tt('compare.level')} ${entry.saved.state.character.level}`, entry.saved.state.character.class_name, entry.saved.state.character.ascendancy_name,
    `${tt('compare.weaponSet')} ${entry.saved.state.weaponSwap?.active ?? 1}`,
    `${tt('compare.treeVersion')} ${resolvedTreeVersion(entry.saved.state.treeVersion, session.dataVersion) ?? tt('compare.defaultVersion')}`].filter(Boolean).join(' · ');
  const pick = (side: 'A' | 'B', id: string) => { (side === 'A' ? setAId : setBId)(id); setTab(null); setMessage(''); };
  return <section className="compare-panel" aria-labelledby="compare-heading">
    <PageHeader id="compare-heading" title={tt('compare.title')} description={tt('compare.hint')} />
    <TabsPrimitive.Root className="cmp-root" value={activeTab} onValueChange={next => setTab(next as CompareTab)}>
      <div className="cmp-bar">
        <div className="cmp-stages">
          {([['A', a], ['B', b]] as const).map(([side, entry]) => <div className="cmp-stage" key={side}>
            <span className="cmp-stage-name"><i className="cmp-mark" data-side={side.toLowerCase()} aria-hidden="true" />{stageWord(side)}</span>
            <AppSelectControl aria-label={stageWord(side)} value={entry.stageId} onChange={event => pick(side, event.target.value)}>
              {entries.map(option => <option key={option.stageId} value={option.stageId}>{option.label}</option>)}
            </AppSelectControl>
            <small className="cmp-identity">{identity(entry)}</small>
          </div>)}
          <div className="cmp-stage cmp-basis">
            <span className="cmp-stage-name">{tt('compare.basis')}</span>
            <AppSelectControl aria-label={tt('compare.basis')} value={basis} onChange={event => setBasis(event.target.value as ConfigBasis)}>
              <option value="own">{tt('compare.own')}</option><option value="a">{tt('compare.useA')}</option>
            </AppSelectControl>
          </div>
        </div>
        <div className="cmp-summary" role="status">
          {loading ? <span>{tt('compare.calculating')}</span> : <>
            {headlines.map(line => <span className="cmp-headline" key={line.id} data-tone={line.tone}>{statDefs.get(line.id)?.label[lang] ?? statNameLabel(lang, line.id)} <b>{signedPercent(line.percent)}</b></span>)}
            <span className="cmp-sentence">{summaryLine(lang, counts)}{treeChanges === null && ` ${tt('compare.treeMismatch')}`}</span></>}
        </div>
        <TabsPrimitive.List className="cmp-tablist" aria-label={words(lang, 'Comparison sections', '对比分区', '比較分區')}>
          {COMPARE_TABS.map(id => <TabsPrimitive.Trigger key={id} value={id} className="cmp-tab">
            {tabLabel[id]}<span className="cmp-count" data-zero={counts[id] === 0 || (id === 'stats' && !value) || undefined}>{id === 'stats' && !value ? '…' : id === 'tree' && treeChanges === null ? '≠' : counts[id]}</span>
          </TabsPrimitive.Trigger>)}
        </TabsPrimitive.List>
      </div>
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
          {sections.map(section => {
            const rows = section.rows.map(def => ({ def, row: value.stats.find(row => row.id === def.id) }))
              .filter(({ row }) => row && (!changedOnly || row.changed));
            if (!rows.length) return null;
            return <div className="compare-table-wrap" key={section.title[lang]}><table><caption>{section.title[lang]}</caption>
              <thead><tr><th scope="col"><span className="sr-only">{tt('compare.summary')}</span></th><th scope="col">A</th><th scope="col">B</th><th scope="col">{tt('compare.delta')}</th><th scope="col">{tt('compare.percent')}</th></tr></thead>
              <tbody>{rows.map(({ def, row: maybe }) => { const row = maybe!; const tone = deltaTone(row.id, row.delta); return <tr key={row.id} data-stat={row.id}>
                <th scope="row">{def.label[lang]}</th>{[row.before, row.after].map((number, i) => <td key={i}>{typeof number === 'number' && Number.isFinite(number) ? formatStatValue(number, def.format, lang) : tt('compare.unavailable')}</td>)}
                <td className="cmp-delta" data-tone={tone}>{formatComparisonDelta(row.delta, def.format, lang)}</td>
                <td className="cmp-delta" data-tone={tone}>{row.percent === null ? '—' : `${row.percent > 0 ? '+' : ''}${row.percent.toFixed(1)}%`}</td>
              </tr>; })}</tbody></table></div>;
          })}
          {changedOnly && counts.stats === 0 && <p className="cmp-none">{tt('compare.none')}</p>}
          <p className="cmp-note">{tt('compare.basisHint')} · {session.dataVersion}</p>
          <details className="cmp-details" open={[value.before, value.after].some(output => output.unsupported_modifiers.length || output.item_errors.length)}><summary>{tt('compare.diagnostics')}</summary>{[value.before, value.after].map((output, index) => <div key={index}>
            <strong>{index === 0 ? 'A' : 'B'} · {tt('sidebar.mainSkill')}: {output.main_skill ? skillDisplayName(output.main_skill.skill_id, lang, catalog) : absent}</strong>
            <ul>{[...output.unsupported_modifiers, ...output.item_errors.map(error => `${error.slot}: ${error.message}`)].map((text, i) => <li key={i}>{text}</li>)}</ul>
          </div>)}</details>
        </article>}
      </TabsPrimitive.Content>

      <TabsPrimitive.Content value="items" className="cmp-panel">
        <h3 className="sr-only">{tt('tab.items')}</h3>
        {itemChanges.map(row => textRow(`${row.section}:${row.slot}`, row.slot, row.before, row.after, { kind: 'item', section: row.section, slot: row.slot }))}
        {weaponChanges.map(row => textRow(`weapon:${row.set}:${row.slot}`, `${tt('compare.weaponSet')} ${row.set} · ${row.slot}`, row.before, row.after, { kind: 'weapon', set: row.set, slot: row.slot }))}
        {!counts.items && <p className="cmp-none">{tt('compare.none')}</p>}
      </TabsPrimitive.Content>

      <TabsPrimitive.Content value="skills" className="cmp-panel">
        <h3 className="sr-only">{tt('tab.skills')}</h3>
        {skillChanges.map(row => <article className="compare-difference" key={row.index}>
          <header><h4>{tt('compare.group')} {row.index + 1}</h4>{row.before?.source || row.after?.source ? <p>{tt('compare.granted')}</p> : (JSON.stringify(row.before) !== JSON.stringify(row.after) || row.mainB) && applyButton({ kind: 'skill', index: row.index }, row.index >= a.saved.state.socketGroups.length)}</header>
          <div className="compare-pair"><div><strong>A</strong>{skill(row.before, row.mainA)}</div><div><strong>B</strong>{skill(row.after, row.mainB)}</div></div>
        </article>)}
        {!counts.skills && <p className="cmp-none">{tt('compare.none')}</p>}
      </TabsPrimitive.Content>

      <TabsPrimitive.Content value="config" className="cmp-panel">
        <h3 className="sr-only">{tt('compare.config')}</h3>
        {configChanges.map(row => <article className="compare-difference" key={row.key} data-config-key={row.key}>
          <header><h4>{row.key}</h4>{applyButton(row.action)}</header><div className="compare-pair"><pre>A · {showValue(row.before)}</pre><pre>B · {showValue(row.after)}</pre></div>
        </article>)}
        {!counts.config && <p className="cmp-none">{tt('compare.none')}</p>}
      </TabsPrimitive.Content>

      <TabsPrimitive.Content value="tree" className="cmp-panel">
        <TreeComparison a={a.saved.state} b={b.saved.state} before={value?.before} after={value?.after}
          dataVersion={session.dataVersion} meta={session.treeMeta} lang={lang} />
      </TabsPrimitive.Content>
    </TabsPrimitive.Root>
  </section>;
}
