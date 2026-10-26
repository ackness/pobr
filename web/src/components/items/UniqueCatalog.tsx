import { AppButton, AppInput } from '../shared/controls';
import { useEffect, useMemo, useRef, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { UniqueDraft, UniqueEntry, UniqueSelection } from '../../api/uniques';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useLocalizedLines } from '../../hooks/useLocalizedLines';
import { slotLabel, type Lang } from '../../lib/i18n';
import { craftT } from '../../lib/itemCraftingText';
import { loadTradeCatalog, type TradeCatalog } from '../../lib/tradeOptimizer';
import { defaultUniqueSelection, filterUniques, uniqueGroups, uniqueOptions, uniquePreviewCatalog, uniqueRolls } from '../../lib/uniqueCatalog';
import { uniqueT } from '../../lib/uniqueCatalogText';
import { AppSelect } from '../shared/AppSelect';
import { AppRange } from '../shared/AppRange';
import { ItemDraftWorkbench } from './ItemDraftWorkbench';
import './itemCrafter.css';

const slots = ['weapon1', 'weapon2', 'helmet', 'bodyarmour', 'gloves', 'boots', 'amulet', 'ring1', 'ring2', 'belt', 'Flask 1', 'Flask 2', 'Charm 1', 'Charm 2', 'Charm 3', 'Jewel@catalogue'];

export function UniqueCatalog({ session, lang }: { session: BuildSession; lang: Lang }) {
  const t = (key: Parameters<typeof uniqueT>[1]) => uniqueT(lang, key);
  const [data, setData] = useState<{ entries: UniqueEntry[]; catalog: TradeCatalog } | null>(null);
  const [error, setError] = useState(false);
  const [query, setQuery] = useState('');
  const [itemClass, setItemClass] = useState('');
  const [slot, setSlot] = useState('');
  const [picked, setPicked] = useState<number | null>(null);
  useEffect(() => {
    let cancelled = false;
    getBackend().then(async backend => {
      const [entries, catalog, game] = await Promise.all([backend.uniqueCatalog(), loadTradeCatalog(), backend.itemCraftingCatalog()]);
      if (!cancelled) setData({ entries, catalog: uniquePreviewCatalog(entries, catalog, game) });
    }).catch(() => { if (!cancelled) setError(true); });
    return () => { cancelled = true; };
  }, []);
  const names = useLocalizedLines(data?.entries.map(entry => entry.name) ?? [], lang);
  const lineLabels = useMemo(() => [...new Set(data?.entries.flatMap(entry => entry.raw.split('\n').map(line => line.replace(/\{[^}]*\}/g, ''))) ?? [])], [data]);
  const translatedLines = useLocalizedLines(lineLabels, lang);
  const localized = useMemo(() => {
    const lines = new Map(lineLabels.map((line, index) => [line, translatedLines[index]]));
    return new Map(data?.entries.map((entry, i) => [entry.id, `${names[i]}\n${entry.raw.split('\n').map(line => lines.get(line.replace(/\{[^}]*\}/g, '')) ?? '').join('\n')}`]) ?? []);
  }, [data, names, lineLabels, translatedLines]);
  const classes = useMemo(() => [...new Set(data?.entries.flatMap(entry => entry.bases.map(base => base.item_class)) ?? [])].sort(), [data]);
  const classNames = useLocalizedLines(classes, lang);
  const results = useMemo(() => data ? filterUniques(data.entries, query, itemClass, slot, data.catalog, localized) : [], [data, query, itemClass, slot, localized]);
  const entry = results.find(entry => entry.id === picked);
  if (error) return <p role="alert">{t('error')}</p>;
  if (!data) return <p role="status">{craftT(lang, 'loading')}</p>;
  return <section className="item-crafter unique-catalogue ui-card" aria-label={t('title')}>
    <p>{t('hint')}</p>
    <div className="item-crafter-fields">
      <label>{t('search')}<AppInput type="search" aria-label={t('search')} value={query} onChange={event => { setQuery(event.target.value); setPicked(null); }} /></label>
      <label>{t('itemClass')}<AppSelect ariaLabel={t('itemClass')} value={itemClass}
        options={[{ value: '', label: t('allClasses') }, ...classes.map((value, i) => ({ value, label: classNames[i] ?? value }))]}
        onChange={value => { setItemClass(value); setPicked(null); }} /></label>
      <label>{t('slot')}<AppSelect ariaLabel={t('slot')} value={slot}
        options={[{ value: '', label: t('allSlots') }, ...slots.map(value => ({ value, label: value.startsWith('Jewel@') ? t('jewel') : slotLabel(lang, value) }))]}
        onChange={value => { setSlot(value); setPicked(null); }} /></label>
    </div>
    <p>{t('matches')}: {results.length}</p>
    <AppSelect ariaLabel={t('choose')} placeholder={t('choose')} value={entry ? String(entry.id) : ''}
      options={results.map(entry => ({ value: String(entry.id), label: names[data.entries.indexOf(entry)] ?? entry.name,
        hint: entry.bases.map(base => base.name).join(' / ') }))} onChange={value => setPicked(Number(value))} />
    {!results.length && <p>{t('empty')}</p>}
    {entry && <UniqueItem key={entry.id} entry={entry} catalog={data.catalog} session={session} lang={lang} />}
  </section>;
}

function UniqueItem({ entry, catalog, session, lang }: { entry: UniqueEntry; catalog: TradeCatalog; session: BuildSession; lang: Lang }) {
  const t = (key: Parameters<typeof uniqueT>[1]) => uniqueT(lang, key);
  const [selection, setSelection] = useState<UniqueSelection>(() => defaultUniqueSelection(entry));
  const [draft, setDraft] = useState<{ value: UniqueDraft; revision: number } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<'error' | 'baseUnavailable' | null>(null);
  const [saved, setSaved] = useState(false);
  const alive = useRef(true);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  const { variants, versions } = uniqueOptions(entry);
  const variantLabels = useLocalizedLines(variants, lang), versionLabels = useLocalizedLines(versions, lang);
  const groups = uniqueGroups(entry, selection.version);
  const rolls = uniqueRolls(entry, selection);
  const labels = useLocalizedLines(rolls.map(line => line.text), lang);
  const update = (value: UniqueSelection) => { setSelection(value); setDraft(null); setError(null); setSaved(false); };
  const prepare = async () => {
    setBusy(true); setError(null);
    try {
      const value = await (await getBackend()).uniqueItemDraft(selection);
      if (alive.current) setDraft(previous => ({ value, revision: (previous?.revision ?? 0) + 1 }));
    } catch (error) {
      if (alive.current) setError(String(error instanceof Error ? error.message : JSON.stringify(error)).includes('unique_base_unavailable') ? 'baseUnavailable' : 'error');
    } finally { if (alive.current) setBusy(false); }
  };
  const previewCatalog = useMemo(() => {
    if (!draft || draft.value.required_level === null) return catalog;
    return { ...catalog, bases: catalog.bases.map(base => base.name === draft.value.base ? {
      ...base, level: Math.max(base.requirement_known === false ? 1 : base.level, draft.value.required_level ?? 1), requirement_known: true,
    } : base) };
  }, [catalog, draft]);
  const base = previewCatalog.bases.find(base => base.name === draft?.value.base);
  const variantOptions = variants.map((_, i) => ({ value: String(i + 1), label: variantLabels[i] ?? variants[i] }));
  return <div className="unique-item">
    <fieldset disabled={busy || session.busy} className="unique-selection">
      <div className="item-crafter-fields">
        {!!versions.length && <label>{t('version')}<AppSelect ariaLabel={t('version')} value={String(selection.version)}
          options={versions.map((name, i) => ({ value: String(i + 1), label: versionLabels[i] ?? name }))}
          onChange={value => update({ ...selection, version: Number(value), rolls: {} })} /></label>}
        {!!variants.length && !groups.length && <label>{t('variant')}<AppSelect ariaLabel={t('variant')} value={String(selection.variant)} options={variantOptions}
          onChange={value => update({ ...selection, variant: Number(value) })} /></label>}
        {!groups.length && selection.alts.map((value, index) => <label key={index}>{t('alt')} {index + 1}
          <AppSelect ariaLabel={`${t('alt')} ${index + 1}`} value={String(value)} options={variantOptions}
            onChange={value => update({ ...selection, alts: selection.alts.map((current, i) => i === index ? Number(value) : current) })} /></label>)}
        {groups.map(group => <label key={group.id}>{t('group')} {group.id}<AppSelect ariaLabel={`${t('group')} ${group.id}`} value={String(selection.groups[group.id])}
          options={variantOptions.filter(option => group.choices.includes(Number(option.value)))}
          onChange={value => update({ ...selection, groups: { ...selection.groups, [group.id]: Number(value) } })} /></label>)}
      </div>
      {!!rolls.length && <div className="unique-rolls"><h4>{t('rolls')}</h4><div className="replacement-actions">
        {([0, .5, 1] as const).map(value => <AppButton key={value} onClick={() => update({ ...selection,
          rolls: { ...selection.rolls, ...Object.fromEntries(rolls.map(line => [line.index, value])) } })}>{t(value === 0 ? 'minimum' : value === 1 ? 'maximum' : 'middle')}</AppButton>)}
      </div>{rolls.map((line, i) => <label key={line.index} className="unique-roll"><span>{labels[i]}</span>
        <div><AppRange min={0} max={100} step={1} aria-label={`${t('roll')} ${i + 1}`} value={line.roll * 100} disabled={busy || session.busy}
          onValueChange={value => update({ ...selection, rolls: { ...selection.rolls, [line.index]: value / 100 } })} /></div><output>{Math.round(line.roll * 100)}%</output>
      </label>)}</div>}
      <AppButton className="trade-primary" onClick={() => void prepare()}>{t(busy ? 'preparing' : 'preview')}</AppButton>
    </fieldset>
    {error && <p role="alert">{t(error)}</p>}
    {draft && base && <ItemDraftWorkbench key={draft.revision} initialText={draft.value.text} base={base} catalog={previewCatalog} session={session} lang={lang} editable={false} />}
    {draft && !base && <div><p>{t('unavailable')}</p><AppButton disabled={saved} onClick={() => {
      session.saveLibraryItem(entry.item_type === 'jewel' ? 'jewel' : 'item', draft.value.text); setSaved(true);
    }}>{craftT(lang, saved ? 'saved' : 'save')}</AppButton></div>}
  </div>;
}
