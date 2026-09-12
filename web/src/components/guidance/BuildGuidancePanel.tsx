import { useEffect, useMemo, useRef, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { GemCatalogEntry, PassiveNode, SocketGroupInput } from '../../api/types';
import { formatApiError } from '../../api/error';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useUpgradeGoal } from '../../hooks/useUpgradeGoal';
import { useItemDisplayNames, useLocalizedLines } from '../../hooks/useLocalizedLines';
import { buildProfile, compareGems, keyPassives, mainGem, matchBuild, ninjaSearchUrl, referenceRequest, resistanceGaps, type BuildReference } from '../../lib/buildGuidance';
import { guidanceT } from '../../lib/guidanceText';
import { slotLabel, statNameLabel, type Lang } from '../../lib/i18n';
import { parsePobColorText } from '../../lib/pobColors';
import { splitNotes } from '../../lib/annotations';
import { gemDisplayName } from '../skills/GemPicker';
import { prettySkillId } from '../skills/SkillsPanel';
import { PageHeader } from '../shared/PageHeader';
import { WeaponSetControl } from '../shared/WeaponSetControl';
import './guidance.css';

type Props = { session: BuildSession; lang: Lang; onSkills: (group: number) => void; onEquipment: () => void; onTree: () => void; onConfig: () => void; onCompareItem: (text: string) => void };

const LEAGUES = [
  ['forbiddenrites', 'Forbidden Rites'], ['forbiddenriteshc', 'HC Forbidden Rites'],
  ['forbiddenritesssf', 'SSF Forbidden Rites'], ['forbiddenriteshcssf', 'HC SSF Forbidden Rites'],
  ['runesofaldur', 'Runes of Aldur'], ['standard', 'Standard'],
] as const;

export function BuildGuidancePanel({ session, lang, onSkills, onEquipment, onTree, onConfig, onCompareItem }: Props) {
  const gt = (key: Parameters<typeof guidanceT>[1]) => guidanceT(lang, key);
  const { goal } = useUpgradeGoal();
  const [catalog, setCatalog] = useState<GemCatalogEntry[]>([]);
  const [nodes, setNodes] = useState<PassiveNode[]>([]);
  const [references, setReferences] = useState<BuildReference[]>([]);
  const [pasted, setPasted] = useState<BuildReference | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [exact, setExact] = useState(false);
  const [league, setLeague] = useState('forbiddenrites');
  const [customLeague, setCustomLeague] = useState('');
  const [sameClass, setSameClass] = useState(true);
  const [loading, setLoading] = useState(true);
  const [failed, setFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const [code, setCode] = useState('');
  const [pasteError, setPasteError] = useState<string | null>(null);
  const [decoding, setDecoding] = useState(false);
  const mounted = useRef(true);
  const detailRef = useRef<HTMLDivElement>(null);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);

  useEffect(() => {
    const controller = new AbortController();
    let active = true;
    setLoading(true); setFailed(false);
    void (async () => {
      const backend = await getBackend();
      const [gems, tree] = await Promise.all([backend.gemCatalog(), backend.loadPassiveTree()]);
      if (!active) return;
      setCatalog(gems); setNodes(tree);
      const response = await fetch(`${import.meta.env.BASE_URL}build-references/index.json`, { signal: controller.signal });
      if (!response.ok) throw new Error('Reference catalog unavailable');
      const data = await response.json();
      if (data.version !== 1 || !Array.isArray(data.references)) throw new Error('Invalid reference catalog');
      const decoded: BuildReference[] = [];
      for (const entry of data.references) {
        if (!active) return;
        decoded.push({ id: entry.id, source: entry.source, build: await backend.decodeBuild(entry.code) });
      }
      if (active) setReferences(decoded);
    })().catch(() => { if (active) setFailed(true); }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; controller.abort(); };
  }, [attempt]);

  const byId = useMemo(() => new Map(catalog.map(gem => [gem.skill_id, gem])), [catalog]);
  const gemName = (id: string) => byId.has(id) ? gemDisplayName(byId.get(id)!, lang) : prettySkillId(id);
  const className = (name: string) => lang === 'en-US' ? name : session.classNames.ascendancies[name] ?? session.classNames.classes[name] ?? name;
  const request = session.currentRequest() ?? {};
  const current = buildProfile(request, byId, session.calc?.main_skill?.skill_id);
  const groupIndex = request.main_socket_group;
  const currentGroup = groupIndex === undefined ? undefined : request.socket_groups?.[groupIndex];
  const all = pasted ? [pasted, ...references] : references;
  const ranked = all.map(reference => {
    const profile = buildProfile(referenceRequest(reference.build), byId,
      reference.build.main_socket_group === null ? null : reference.build.socket_groups[reference.build.main_socket_group]?.active_skill_id);
    return { reference, profile, match: matchBuild(current, profile) };
  }).filter(({ profile, match }) => (!exact || match.sameSkill) &&
    [profile.className, className(profile.className), profile.ascendancy, className(profile.ascendancy),
      profile.mainSkill ? gemName(profile.mainSkill) : '', profile.mainSkill ? byId.get(profile.mainSkill)?.name : '', ...profile.uniques]
      .join(' ').toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()))
    .sort((a, b) => b.match.score - a.match.score || a.reference.id.localeCompare(b.reference.id));
  const selected = all.find(reference => reference.id === selectedId);
  const selectedGroup = selected?.build.main_socket_group === null || !selected ? undefined : selected.build.socket_groups[selected.build.main_socket_group];
  const gaps = resistanceGaps(session.busy ? null : session.calc, goal.resistanceTarget ?? 75);
  const searchUrl = ninjaSearchUrl(league === 'custom' ? customLeague.trim() : league, current, byId, sameClass);
  const referenceNotes = splitNotes(selected?.build.notes ?? '').overview;
  useEffect(() => {
    if (!selectedId) return;
    detailRef.current?.focus({ preventScroll: true });
    detailRef.current?.scrollIntoView({ block: 'start' });
  }, [selectedId]);
  const open = (id: string) => {
    setSelectedId(id);
    if (id === selectedId) detailRef.current?.scrollIntoView({ block: 'start' });
  };
  const paste = async () => {
    setDecoding(true); setPasteError(null);
    try {
      const build = await (await getBackend()).decodeBuild(code.trim());
      if (!mounted.current) return;
      setPasted({ id: 'pasted', source: {}, build });
      open('pasted');
    } catch (error) { if (mounted.current) setPasteError(formatApiError(error)); }
    finally { if (mounted.current) setDecoding(false); }
  };

  return <section className="ui-page guidance-page" aria-labelledby="guidance-heading">
    <PageHeader id="guidance-heading" title={gt('title')} description={gt('hint')} eyebrow="POE.NINJA × PoBR">
      <WeaponSetControl session={session} lang={lang} />
    </PageHeader>
    <div className="guidance-identity">
      <div><span className="page-eyebrow">{gt('identity')}</span><h3>{current.mainSkill ? gemName(current.mainSkill) : gt('noSkill')}</h3>
        <p>Lv {session.character?.level} · {className(current.ascendancy || current.className)}</p></div>
      <label>{gt('skills')}<select aria-label={gt('skills')} value={groupIndex ?? ''} disabled={session.busy}
        onChange={event => session.updateParams({ main_socket_group: Number(event.target.value) })}>
        {groupIndex === undefined && <option value="">—</option>}
        {request.socket_groups?.map((group, index) => { const gem = mainGem(group, byId); return gem &&
          <option value={index} key={index}>{index + 1}. {gemName(gem.skill_id)}</option>; })}
      </select></label>
    </div>
    <h3>{gt('next')}</h3>
    {(!!session.calc?.unsupported_modifiers.length || !!session.calc?.item_errors.length) && <p className="guidance-notice">{gt('partial')}</p>}
    <div className="guidance-actions">
      <article className="guidance-card"><span className="guidance-step">01</span><h4>{gt('skills')}</h4><p>{gt('skillHint')}</p>
        <button disabled={groupIndex === undefined || session.busy} onClick={() => groupIndex !== undefined && onSkills(groupIndex)}>{gt('calculate')}</button></article>
      <article className="guidance-card"><span className="guidance-step">02</span><h4>{gaps.length ? gt('gaps') : gt('gear')}</h4>
        {gaps.length > 0 && <ul className="guidance-gap-list">{gaps.map(gap => <li key={gap.stat}>{statNameLabel(lang, gap.stat)} <strong>{gap.value.toFixed(0)}% → {gap.target}%</strong></li>)}</ul>}
        <p>{gaps.length ? gt('gapHint') : gt('gearHint')}</p><button onClick={onEquipment}>{gt('calculate')}</button></article>
      <article className="guidance-card"><span className="guidance-step">03</span><h4>{gt('tree')}</h4><p>{gt('treeHint')}</p>
        <button onClick={onTree}>{gt('calculate')}</button></article>
    </div>
    <button className="guidance-config" onClick={onConfig}>{gt('configure')} →</button>

    <article className="guidance-card guidance-live"><div><h3>{gt('live')}</h3><p>{gt('leagueHint')}</p></div>
      <div className="guidance-filters"><label>{gt('league')}<select value={league} onChange={event => setLeague(event.target.value)}>
        {LEAGUES.map(([id, name]) => <option key={id} value={id}>{name}</option>)}<option value="custom">{gt('otherLeague')}</option>
      </select></label>
        {league === 'custom' && <label>{gt('leagueCode')}<input value={customLeague} onChange={event => setCustomLeague(event.target.value)} spellCheck={false} aria-invalid={!searchUrl} /></label>}
        <label className="guidance-checkbox"><input type="checkbox" checked={sameClass} onChange={event => setSameClass(event.target.checked)} />{gt('sameClass')}</label>
        {searchUrl && <a className="guidance-link" href={searchUrl} target="_blank" rel="noreferrer">{gt('searchLive')} ↗</a>}
        <a href="https://poe.ninja/poe2/builds/" target="_blank" rel="noreferrer">{gt('leagues')} ↗</a></div>
    </article>
    <div className="guidance-section-heading"><h3>{gt('samples')} <span>{ranked.length}</span></h3>
      <details><summary>{gt('score')}</summary><p>{gt('formula')}</p></details></div>
    <p className="guidance-notice">{gt('history')}</p>
    <div className="guidance-filters"><input className="guidance-search" aria-label={gt('search')} placeholder={gt('search')} value={query} onChange={event => setQuery(event.target.value)} />
      <label className="guidance-checkbox"><input type="checkbox" checked={exact} onChange={event => setExact(event.target.checked)} />{gt('exact')}</label></div>
    {loading && <p role="status">{gt('loading')}</p>}
    {failed && <p role="alert">{gt('failed')} <button onClick={() => setAttempt(value => value + 1)}>{gt('retry')}</button></p>}
    {!loading && !ranked.length && <p className="guidance-empty">{gt('empty')}</p>}
    {!!ranked.length && ranked[0].match.score === 0 && <p className="guidance-notice">{gt('notRelated')}</p>}
    <div className="guidance-results">{ranked.map(({ reference, profile, match }) => <article className={`guidance-card guidance-result${selectedId === reference.id ? ' is-selected' : ''}`} key={reference.id}>
      <div className="guidance-result-top"><div><h4>{profile.mainSkill ? gemName(profile.mainSkill) : '—'}</h4><p>{className(profile.ascendancy || profile.className)} · Lv {reference.build.character.level}</p></div>
        <div className="guidance-score"><strong>{match.score}</strong><span>{gt('score')} / 100</span></div></div>
      <div className="guidance-badges"><span>{gt(match.sameSkill ? 'sameSkill' : 'otherSkill')}</span>
        {match.sameAscendancy ? <span>{gt('ascendancy')}</span> : match.sameClass && <span>{gt('baseClass')}</span>}
        {match.supportOverlap > 0 && <span>{gt('supports')} {Math.round(match.supportOverlap * 100)}%</span>}
        {match.uniqueOverlap > 0 && <span>{gt('uniques')} {Math.round(match.uniqueOverlap * 100)}%</span>}</div>
      <p className="guidance-source">{reference.source.url ? `${reference.source.league} · ${reference.source.gameVersion} · ${reference.source.fetchedAt}` : gt('pasted')}</p>
      <div className="guidance-row-actions"><button aria-pressed={selectedId === reference.id} onClick={() => open(reference.id)}>{gt('details')}</button>
        {reference.source.url && <a href={reference.source.url} target="_blank" rel="noreferrer">{gt('source')} ↗</a>}</div>
    </article>)}</div>
    <article className="guidance-card guidance-paste"><h3>{gt('pasteTitle')}</h3><p>{gt('pasteHint')}</p>
      <textarea aria-label={gt('paste')} placeholder={gt('paste')} value={code} onChange={event => setCode(event.target.value)} rows={3} spellCheck={false} />
      <button onClick={() => void paste()} disabled={!code.trim() || decoding}>{decoding ? gt('loading') : gt('compare')}</button>
      {pasteError && <p role="alert">{pasteError}</p>}
    </article>
    {selected && <div ref={detailRef} className="guidance-detail" tabIndex={-1} aria-label={gt('details')}>
      <PageHeader id="reference-heading" title={`${gt('reference')} · ${selectedGroup?.active_skill_id ? gemName(selectedGroup.active_skill_id) : className(selected.build.character.ascendancy_name)}`} description={gt('differences')} />
      <h3>{gt('skills')}</h3><p className="guidance-muted">{gt('mainOnly')}</p>
      <div className="guidance-table-wrap"><table className="guidance-table"><thead><tr><th>{gt('gem')}</th><th>{gt('current')}</th><th>{gt('reference')}</th></tr></thead>
        <tbody>{compareGems(currentGroup?.gems ?? [], selectedGroup?.gems ?? []).map(row => <tr key={row.id}>
          <th><span className={`guidance-gem guidance-gem--${byId.get(row.id)?.colour ?? 'none'}`}>◆</span> {gemName(row.id)}</th>
          {[row.current, row.reference].map((gem, index) => <td key={index}>{gem ? `${gt('level')} ${gem.level} · ${gt('quality')} ${gem.quality}%` : gt('absent')}</td>)}</tr>)}</tbody></table></div>
      <details className="guidance-card"><summary>{gt('reference')} · {gt('skills')} ({selected.build.socket_groups.length})</summary>
        {selected.build.socket_groups.map((group, index) => <ReferenceSkillGroup key={index} group={group} index={index} gemName={gemName} />)}</details>
      <h3>{gt('gear')}</h3><p className="guidance-muted">{gt('gearHint')}</p>
      <EquipmentComparison session={session} reference={selected} lang={lang} onCompareItem={onCompareItem} />
      <h3>{gt('tree')}</h3><p className="guidance-notice">{gt('passiveVersion')}</p>
      <div className="guidance-passives"><PassiveList title={gt('current')} nodes={keyPassives(session.allocatedNodes, nodes)} lang={lang} />
        <PassiveList title={gt('reference')} nodes={keyPassives(selected.build.tree.allocated_nodes, nodes)} lang={lang} /></div>
      <details className="guidance-card"><summary>{gt('notes')}</summary><p>{gt('notesHint')}</p><pre className="guidance-notes">{referenceNotes ? parsePobColorText(referenceNotes).map(part => part.text).join('') : gt('noNotes')}</pre></details>
    </div>}
  </section>;
}

function ReferenceSkillGroup({ group, index, gemName }: { group: SocketGroupInput; index: number; gemName: (id: string) => string }) {
  return <p className={`guidance-skill-group${group.enabled ? '' : ' is-disabled'}`}><strong>{index + 1}.</strong> {group.gems.map(gem => `${gemName(gem.skill_id)} (${gem.level} / ${gem.quality}%)`).join(' · ')}</p>;
}

function EquipmentComparison({ session, reference, lang, onCompareItem }: { session: BuildSession; reference: BuildReference; lang: Lang; onCompareItem: (text: string) => void }) {
  const gt = (key: Parameters<typeof guidanceT>[1]) => guidanceT(lang, key);
  const current = [...session.items, ...session.flasks];
  const other = [...reference.build.items.equipped, ...reference.build.items.flasks];
  const slots = [...new Set([...other, ...current].map(item => item.slot))];
  const texts = slots.flatMap(slot => [current.find(item => item.slot === slot)?.text, other.find(item => item.slot === slot)?.text]);
  const names = useItemDisplayNames(texts, lang);
  return <div className="guidance-equipment">{slots.map((slot, index) => <article className="guidance-card" key={slot}><h4>{slotLabel(lang, slot)}</h4>
    <div className="guidance-equipment-pair">{[0, 1].map(side => <div key={side}><span className="page-eyebrow">{gt(side ? 'reference' : 'current')}</span>
      <p>{names[index * 2 + side] || gt('absent')}</p>{texts[index * 2 + side] && <><details><summary>{gt('text')}</summary><pre>{texts[index * 2 + side]}</pre></details>
        {side === 1 && <button disabled={session.busy} onClick={() => onCompareItem(texts[index * 2 + side]!)}>{gt('evaluateItem')}</button>}</>}</div>)}</div></article>)}</div>;
}

function PassiveList({ title, nodes, lang }: { title: string; nodes: PassiveNode[]; lang: Lang }) {
  const translated = useLocalizedLines(nodes.flatMap(node => [node.name ?? node.id, ...(node.stats ?? [])]), lang);
  let offset = 0;
  return <article className="guidance-card"><h4>{title} ({nodes.length})</h4>{!nodes.length && <p>{guidanceT(lang, 'noPassives')}</p>}
    {nodes.map(node => { const name = translated[offset++]; const lines = translated.slice(offset, offset + (node.stats?.length ?? 0)); offset += lines.length;
      return <details key={node.skill}><summary>{name}</summary><ul>{lines.map((line, index) => <li key={index}>{line}</li>)}</ul></details>; })}</article>;
}
