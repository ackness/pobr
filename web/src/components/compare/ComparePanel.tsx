import { useEffect, useMemo, useState } from 'react';
import type { BuildSession } from '../../hooks/useBuildSession';
import { toRequest } from '../../hooks/useBuildSession';
import { useSkillCatalog } from '../../hooks/useSkillName';
import { formatApiError } from '../../api/error';
import type { SocketGroupInput } from '../../api/types';
import { allocationDiff, compareBuilds, configDifferences, copySection, diffText, formatComparisonDelta, itemDifferences, resolvedTreeVersion, skillDifferences, weaponDifferences, type ConfigBasis, type CopySection } from '../../lib/compare';
import { bindT, t, type Lang } from '../../lib/i18n';
import { skillDisplayName } from '../../lib/skillNames';
import { STAT_SECTIONS, formatStatValue } from '../../lib/statDisplay';
import { PageHeader } from '../shared/PageHeader';
import './compare.css';

export function ComparePanel({ session, lang }: { session: BuildSession; lang: Lang }) {
  const tt = bindT(lang);
  const catalog = useSkillCatalog();
  const entries = useMemo(() => session.workspace?.builds.flatMap(build => build.stages.map(stage => ({
    buildId: build.id, stageId: stage.id, saved: stage.saved, drafts: stage.drafts,
    label: `${build.name || tt('compare.build')} / ${stage.name || tt('compare.stage')}`,
  }))) ?? [], [session.workspace, lang]);
  const active = session.workspace?.builds.find(build => build.id === session.workspace?.activeBuild)?.activeStage;
  const [aId, setAId] = useState(active);
  const [bId, setBId] = useState(entries.find(entry => entry.stageId !== active)?.stageId);
  const [basis, setBasis] = useState<ConfigBasis>('own');
  const [changedOnly, setChangedOnly] = useState(true);
  const [message, setMessage] = useState('');
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
  const applyButton = (section: CopySection, append = false) => <button type="button" disabled={!canApply}
    onClick={() => apply(section)}>{tt(append ? 'compare.append' : 'compare.apply')}</button>;
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
    return <article className="compare-difference" key={id} data-item={id}>
      <header><h4>{title}</h4>{applyButton(section)}</header>
      <div className="compare-pair">{(['before', 'after'] as const).map((side, index) => <div key={side}><strong>{index === 0 ? 'A' : 'B'}</strong>
        <pre>{lines[side].length ? lines[side].map((line, i) => <span key={i} className={line.changed ? `compare-line-${side}` : ''}>{line.changed ? index ? '+ ' : '− ' : '  '}{line.text}{'\n'}</span>) : absent}</pre>
      </div>)}</div>
    </article>;
  };
  return <section className="compare-panel" aria-labelledby="compare-heading">
    <PageHeader id="compare-heading" title={tt('compare.title')} description={tt('compare.hint')} />
    {entries.length < 2 ? <p>{tt('compare.needStages')}</p> : <>
      <div className="compare-controls">
        {(['A', 'B'] as const).map(side => <label key={side}>{tt(side === 'A' ? 'compare.a' : 'compare.b')}
          <select aria-label={tt(side === 'A' ? 'compare.a' : 'compare.b')} value={side === 'A' ? a.stageId : b.stageId}
            onChange={event => { (side === 'A' ? setAId : setBId)(event.target.value); setMessage(''); }}>
            {entries.map(entry => <option key={entry.stageId} value={entry.stageId}>{entry.label}</option>)}
          </select></label>)}
        <label>{tt('compare.basis')}<select value={basis} onChange={event => setBasis(event.target.value as ConfigBasis)}>
          <option value="own">{tt('compare.own')}</option><option value="a">{tt('compare.useA')}</option>
        </select></label>
      </div>
      <p className="compare-hint">{tt('compare.basisHint')} · {session.dataVersion}</p>
      <div className="compare-pair compare-identities">{[a, b].map((entry, index) => <div key={index}>
        <strong>{index === 0 ? 'A' : 'B'} · {entry.label}</strong><p>{tt('compare.level')} {entry.saved.state.character.level} · {entry.saved.state.character.class_name} · {entry.saved.state.character.ascendancy_name}</p>
        <p>{tt('compare.weaponSet')} {entry.saved.state.weaponSwap?.active ?? 1} · {tt('compare.treeVersion')} {resolvedTreeVersion(entry.saved.state.treeVersion, session.dataVersion) ?? tt('compare.defaultVersion')}</p>
      </div>)}</div>
      {(Object.keys(a.drafts).length > 0 || Object.keys(b.drafts).length > 0) && <p>{tt('compare.draftHint')}</p>}
      {drafts && <p role="note">{tt('compare.drafts')}</p>}
      {message && <p role="status">{message}</p>}
      {loading && <p role="status">{tt('compare.calculating')}</p>}
      {current?.error && <p role="alert">{current.error}</p>}
      {value && <article className="compare-summary">
        <h3>{tt('compare.summary')}</h3><label><input type="checkbox" checked={changedOnly} onChange={event => setChangedOnly(event.target.checked)} /> {tt('compare.changed')}</label>
        <p className="compare-hint">{tt('compare.percentHint')}</p>
        {sections.map(section => {
          const rows = section.rows.map(def => ({ def, row: value.stats.find(row => row.id === def.id) }))
            .filter(({ row }) => row && (!changedOnly || row.changed));
          if (!rows.length) return null;
          return <div className="compare-table-wrap" key={section.title[lang]}><table><caption>{section.title[lang]}</caption>
            <thead><tr><th scope="col">{tt('compare.summary')}</th><th scope="col">A</th><th scope="col">B</th><th scope="col">{tt('compare.delta')}</th><th scope="col">{tt('compare.percent')}</th></tr></thead>
            <tbody>{rows.map(({ def, row: maybe }) => { const row = maybe!; return <tr key={row.id} data-stat={row.id}>
              <th scope="row">{def.label[lang]}</th>{[row.before, row.after].map((number, i) => <td key={i}>{typeof number === 'number' && Number.isFinite(number) ? formatStatValue(number, def.format, lang) : tt('compare.unavailable')}</td>)}
              <td>{formatComparisonDelta(row.delta, def.format, lang)}</td>
              <td>{row.percent === null ? '—' : `${row.percent > 0 ? '+' : ''}${row.percent.toFixed(1)}%`}</td>
            </tr>; })}</tbody></table></div>;
        })}
        {changedOnly && !value.stats.some(row => row.changed) && <p>{tt('compare.none')}</p>}
        <details open={[value.before, value.after].some(output => output.unsupported_modifiers.length || output.item_errors.length)}><summary>{tt('compare.diagnostics')}</summary>{[value.before, value.after].map((output, index) => <div key={index}>
          <strong>{index === 0 ? 'A' : 'B'} · {tt('sidebar.mainSkill')}: {output.main_skill ? skillDisplayName(output.main_skill.skill_id, lang, catalog) : absent}</strong>
          <ul>{[...output.unsupported_modifiers, ...output.item_errors.map(error => `${error.slot}: ${error.message}`)].map((text, i) => <li key={i}>{text}</li>)}</ul>
        </div>)}</details>
      </article>}
      <section><h3>{tt('tab.items')}</h3>
        {itemDifferences(a.saved.state, b.saved.state).map(row => textRow(`${row.section}:${row.slot}`, row.slot, row.before, row.after, { kind: 'item', section: row.section, slot: row.slot }))}
        {weaponDifferences(a.saved.state, b.saved.state).map(row => textRow(`weapon:${row.set}:${row.slot}`, `${tt('compare.weaponSet')} ${row.set} · ${row.slot}`, row.before, row.after, { kind: 'weapon', set: row.set, slot: row.slot }))}
        {!itemDifferences(a.saved.state, b.saved.state).length && !weaponDifferences(a.saved.state, b.saved.state).length && <p>{tt('compare.none')}</p>}
      </section>
      <section><h3>{tt('tab.skills')}</h3>{skillDifferences(a.saved.state, b.saved.state).map(row => <article className="compare-difference" key={row.index}>
        <header><h4>{tt('compare.group')} {row.index + 1}</h4>{row.before?.source || row.after?.source ? <p>{tt('compare.granted')}</p> : (JSON.stringify(row.before) !== JSON.stringify(row.after) || row.mainB) && applyButton({ kind: 'skill', index: row.index }, row.index >= a.saved.state.socketGroups.length)}</header>
        <div className="compare-pair"><div><strong>A</strong>{skill(row.before, row.mainA)}</div><div><strong>B</strong>{skill(row.after, row.mainB)}</div></div>
      </article>)}{!skillDifferences(a.saved.state, b.saved.state).length && <p>{tt('compare.none')}</p>}</section>
      <section><h3>{tt('compare.config')}</h3>{[
        ...configDifferences(a.saved.state, b.saved.state).map(row => ({ ...row, action: { kind: 'config', key: row.key } as CopySection })),
        { key: tt('compare.enemy'), before: a.saved.state.params.enemy_tier, after: b.saved.state.params.enemy_tier, action: { kind: 'enemy' } as CopySection },
        { key: tt('compare.mods'), before: a.saved.state.params.custom_modifier_blocks ?? a.saved.state.params.extra_modifiers,
          after: b.saved.state.params.custom_modifier_blocks ?? b.saved.state.params.extra_modifiers, action: { kind: 'modifiers' } as CopySection },
      ].filter(row => JSON.stringify(row.before) !== JSON.stringify(row.after)).map(row => <article className="compare-difference" key={row.key} data-config-key={row.key}>
        <header><h4>{row.key}</h4>{applyButton(row.action)}</header><div className="compare-pair"><pre>A · {showValue(row.before)}</pre><pre>B · {showValue(row.after)}</pre></div>
      </article>)}</section>
      <section><h3>{tt('compare.passives')}</h3>
        {resolvedTreeVersion(a.saved.state.treeVersion, session.dataVersion) !== resolvedTreeVersion(b.saved.state.treeVersion, session.dataVersion) && <p role="note">{tt('compare.treeMismatch')}</p>}
        <p>A: {new Set(a.saved.state.allocatedNodes).size} · B: {new Set(b.saved.state.allocatedNodes).size}</p>
        {Object.entries(allocationDiff(a.saved.state.allocatedNodes, b.saved.state.allocatedNodes)).map(([kind, nodes]) => <details key={kind}>
          <summary>{tt(kind === 'added' ? 'compare.added' : kind === 'removed' ? 'compare.removed' : 'compare.both')} ({nodes.length})</summary><p>{nodes.join(', ') || '—'}</p>
        </details>)}
      </section>
    </>}
  </section>;
}
