import { useEffect, useMemo, useRef, useState } from 'react';
import type { ReplacementReport } from '../../lib/itemReplacement';
import type { Lang } from '../../lib/i18n';
import type { TradeCatalog } from '../../lib/tradeOptimizer';
import { useLocalizedLines } from '../../hooks/useLocalizedLines';
import { affixRollLines, affixTier, availableAffixes, buildAffixItem, createAffixDraft, editableAffixPool, validateAffixDraft, type AffixDraft } from '../../lib/marketAffixEditor';
import { affixT } from '../../lib/marketAffixText';
import { AppSelect } from '../shared/AppSelect';
import './replacementAffixEditor.css';

/** Draft edits are local to the candidate; only the caller can run or apply a comparison. */
export function ReplacementAffixEditor({ report, catalog, characterLevel, lang, disabled, onCompare, onDirtyChange }: {
  report: ReplacementReport; catalog: TradeCatalog; characterLevel: number; lang: Lang; disabled: boolean;
  onCompare: (text: string) => void; onDirtyChange: (dirty: boolean) => void;
}) {
  const t = (key: Parameters<typeof affixT>[1]) => affixT(lang, key);
  const pool = useMemo(() => editableAffixPool(catalog, report.base), [catalog, report.base]);
  const labels = useMemo(() => [...new Set(pool.flatMap(mod => mod.lines))], [pool]);
  const translated = useLocalizedLines(labels, lang);
  const aliases = useMemo(() => new Map(labels.map((line, index) => [line, translated[index]])), [labels, translated]);
  const tiers = useMemo(() => new Map(pool.map(mod => [mod.id, affixTier(mod, pool)])), [pool]);
  const initial = useMemo(() => createAffixDraft(report.text, report.base, report.lines, pool, aliases), [report.text, report.base, report.lines, pool, aliases]);
  const [draft, setDraft] = useState<AffixDraft | null>(null);
  const [query, setQuery] = useState('');
  const [kind, setKind] = useState<'prefix' | 'suffix'>('prefix');
  const [picked, setPicked] = useState('');
  const [replacing, setReplacing] = useState<string | undefined>();
  const current = draft ?? initial;
  const [error, setError] = useState('');
  const nextId = useRef(0);
  const selectedMods = current.rows.flatMap(row => pool.find(mod => mod.id === row.modId) ?? []);
  const issue = validateAffixDraft(current, pool, characterLevel);
  const options = availableAffixes(current, pool, characterLevel, replacing)
    .filter(mod => mod.kind === kind && `${mod.id} ${mod.lines.join(' ')} ${mod.lines.map(line => aliases.get(line)).join(' ')}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()))
    .sort((a, b) => a.group.localeCompare(b.group) || b.level - a.level || a.id.localeCompare(b.id));
  const choice = options.find(mod => mod.id === picked);
  const selectedLines = current.rows.flatMap(row => row.edited && row.modId
    ? affixRollLines(pool.find(mod => mod.id === row.modId)!, row.roll) : row.original);
  const displayed = useLocalizedLines(selectedLines, lang);
  const previewLines = choice ? affixRollLines(choice) : [];
  const preview = useLocalizedLines(previewLines, lang);
  useEffect(() => () => onDirtyChange(false), [onDirtyChange]);
  const update = (value: AffixDraft) => { setDraft(value); setError(''); onDirtyChange(true); };
  let lineOffset = 0;
  const add = () => {
    if (!choice) return;
    const row = { key: replacing ?? `new-${++nextId.current}`, modId: choice.id,
      indices: [], original: [], roll: 0.5, edited: true };
    update({ ...current, rows: replacing ? current.rows.map(existing => existing.key === replacing ? row : existing) : [...current.rows, row] });
    setReplacing(undefined); setPicked('');
  };
  return <details className="replacement-affix-editor">
    <summary>{t('title')}</summary>
    <p className="affix-editor-hint">{t('hint')}</p>
    {current.locked ? <p className="affix-editor-warning">{t(current.locked)}</p> : !pool.length ? <p>{t('dataMissing')}</p> : <>
      <div className="affix-editor-meta"><label>{t('itemLevel')}<input type="number" min="1" max="100" value={current.itemLevel ?? ''} disabled={disabled}
        onChange={event => update({ ...current, itemLevel: event.target.value === '' ? null : Number(event.target.value) })} /></label>
        {(['prefix', 'suffix'] as const).map(type => <span key={type}>{t(type)} <b>{selectedMods.filter(mod => mod.kind === type).length} / {current.limit}</b></span>)}
      </div>
      {current.itemLevel === null && <p className="affix-editor-warning">{t('missingLevel')}</p>}
      {current.rows.some(row => !row.modId) && <p className="affix-editor-warning">{t('unknown')}</p>}
      <div className="affix-editor-rows">{current.rows.map(row => {
        const mod = pool.find(candidate => candidate.id === row.modId);
        const length = row.edited && mod ? mod.lines.length : row.original.length;
        const lines = displayed.slice(lineOffset, lineOffset + length); lineOffset += length;
        return <article className="affix-editor-row" key={row.key} data-affix-id={row.modId}>
          <div className="affix-editor-row-heading"><span>{mod ? `${t(mod.kind)} · T${tiers.get(mod.id)}` : t('unresolved')}</span>
            <div><button disabled={disabled || row.fractured} onClick={() => { setReplacing(row.key); if (mod) setKind(mod.kind); setPicked(''); }}>{t('replace')}</button>
              <button disabled={disabled || row.fractured} aria-label={`${t('remove')}: ${lines.join(' ')}`} onClick={() => { update({ ...current, rows: current.rows.filter(existing => existing.key !== row.key) }); if (replacing === row.key) setReplacing(undefined); }}>{t('remove')}</button></div></div>
          {lines.map((line, index) => <p key={index}>{line}</p>)}
          {row.fractured && <p className="affix-editor-hint">{t('fractured')}</p>}
          {mod && !row.fractured && <div className="affix-editor-roll"><span>{row.edited ? `${t('range')} · ${Math.round(row.roll * 100)}%` : t('current')}</span>
            <div>{([0, 0.5, 1] as const).map(roll => <button key={roll} disabled={disabled} aria-pressed={row.edited && row.roll === roll}
              onClick={() => update({ ...current, rows: current.rows.map(existing => existing.key === row.key ? { ...existing, edited: true, roll } : existing) })}>{t(roll === 0 ? 'minimum' : roll === 1 ? 'maximum' : 'midpoint')}</button>)}</div>
            {row.edited && <input type="range" min="0" max="100" step="1" value={row.roll * 100} disabled={disabled} aria-label={`${t('range')}: ${mod.id}`}
              onChange={event => update({ ...current, rows: current.rows.map(existing => existing.key === row.key ? { ...existing, roll: Number(event.target.value) / 100 } : existing) })} />}
          </div>}
        </article>;
      })}</div>
      <div className="affix-editor-add">
        <div className="affix-editor-kind" role="group" aria-label={t('choose')}>{(['prefix', 'suffix'] as const).map(type => <button key={type} aria-pressed={kind === type} disabled={disabled}
          onClick={() => { setKind(type); setPicked(''); }}>{t(type)}</button>)}</div>
        {replacing && <p>{t('replace')} <button onClick={() => { setReplacing(undefined); setPicked(''); }}>{t('reset')}</button></p>}
        <input type="search" value={query} placeholder={t('search')} aria-label={t('search')} onChange={event => setQuery(event.target.value)} />
        <AppSelect value={choice?.id ?? ''} placeholder={t('choose')} ariaLabel={t('choose')} disabled={disabled || !options.length}
          options={options.map(mod => ({ value: mod.id, label: `${mod.lines.map(line => aliases.get(line) ?? line).join(' / ')} · T${tiers.get(mod.id)}`,
            hint: `${t('itemLevel')} ${mod.level} · ${t('requirement')} ${Math.max(report.base.level, Math.floor(mod.level * 0.8))}` }))} onChange={setPicked} />
        {!options.length && <p className="affix-editor-hint">{t('noOptions')}</p>}
        {!!preview.length && <div className="affix-editor-preview"><small>{t('midpoint')}</small>{preview.map((line, index) => <p key={index}>{line}</p>)}</div>}
        <button className="trade-primary" disabled={disabled || !choice} onClick={add}>{t(replacing ? 'replace' : 'add')}</button>
      </div>
      <p className="affix-editor-hint">{t('estimate')}</p>
      {!!draft && issue && issue !== 'unknown' && <p className="affix-editor-warning">{t(issue as Parameters<typeof affixT>[1])}</p>}
      {error && <p role="alert" className="affix-editor-warning">{error}</p>}
      <div className="affix-editor-actions"><button className="trade-primary" disabled={disabled || !draft || !!issue} onClick={() => {
        try { onCompare(buildAffixItem(current, pool, characterLevel)); } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); }
      }}>{t('compare')}</button><button disabled={!draft || disabled} onClick={() => { setDraft(null); setReplacing(undefined); setPicked(''); setError(''); onDirtyChange(false); }}>{t('reset')}</button></div>
    </>}
  </details>;
}
