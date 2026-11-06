import { AppButton, AppInput } from '../shared/controls';
import { useEffect, useMemo, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useLocalizedLines, useLocalizedNames } from '../../hooks/useLocalizedLines';
import { shownLines } from '../../lib/continuedLines';
import type { Lang } from '../../lib/i18n';
import { baseDraftText, craftBaseDisplayName, craftingCatalog, defaultCraftClass, itemClassName, itemClassLabel, itemNameLabel, type CraftCatalog } from '../../lib/itemCrafting';
import { craftT } from '../../lib/itemCraftingText';
import { loadTradeCatalog } from '../../lib/tradeOptimizer';
import { AppSelect } from '../shared/AppSelect';
import { ItemDraftWorkbench } from './ItemDraftWorkbench';
import './itemCrafter.css';

interface EquippedItemDraft {
  source: string;
  text: string;
  catalog: CraftCatalog;
  base: CraftCatalog['bases'][number] | undefined;
}

/** Resolve only the item header, preserving every property and modifier line. */
function craftingHeader(text: string, catalog: CraftCatalog) {
  const lines = text.split('\n');
  const rarityIndex = lines.findIndex(line => /^Rarity:\s*(NORMAL|MAGIC|RARE|UNIQUE|RELIC)\s*$/i.test(line.trim()));
  if (rarityIndex < 0) return;
  const rarity = lines[rarityIndex].trim().split(/:\s*/)[1].toUpperCase();
  const headers = lines.flatMap((line, index) => index > rarityIndex && line.trim() ? [index] : []);
  const index = headers[rarity === 'NORMAL' || rarity === 'MAGIC' ? 0 : 1];
  if (index === undefined) return;
  const name = lines[index].trim();
  const base = catalog.bases.find(base => base.name === name)
    ?? (rarity === 'MAGIC' ? [...catalog.bases].sort((a, b) => b.name.length - a.name.length).find(base =>
      name.endsWith(` ${base.name}`) || name.startsWith(`${base.name} of `)
      || name.includes(` ${base.name} of `)) : undefined);
  if (!base) return;
  // The normalizer accepts exact base names, including for decorated magic items.
  lines[index] = base.name;
  return { text: lines.join('\n'), base };
}

/** Prepare the selected item before enabling its crafter entry. */
export function useEquippedItemDraft(text: string | undefined) {
  const [loaded, setLoaded] = useState<EquippedItemDraft | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    setLoaded(null);
    setFailed(null);
    if (!text) return;
    Promise.all([getBackend().then(async backend => ({ backend, game: await backend.itemCraftingCatalog() })),
      loadTradeCatalog()]).then(async ([{ backend, game }, display]) => {
      if (cancelled) return;
      const catalog = craftingCatalog(game, display);
      const header = craftingHeader(text, catalog);
      if (!header) {
        setLoaded({ source: text, text, catalog, base: undefined });
        return;
      }
      const normalized = await backend.normalizeItemDraft(header.text);
      if (!cancelled) setLoaded({ source: text, text: normalized, catalog, base: header.base });
    }).catch(() => { if (!cancelled) setFailed(text); });
    return () => { cancelled = true; };
  }, [text]);
  return { draft: loaded?.source === text ? loaded : null, error: !!text && failed === text };
}

/** Edit a copy of the equipped item; the workbench previews changes before applying them. */
export function EquippedItemCrafter({ draft, session, lang }: { draft: EquippedItemDraft; session: BuildSession; lang: Lang }) {
  if (!draft.base) return <p className="items-hint">{craftT(lang, 'editUnavailable')}</p>;
  return <section className="item-crafter ui-card" aria-label={craftT(lang, 'craft')}>
    <ItemDraftWorkbench initialText={draft.text} base={draft.base} catalog={draft.catalog} session={session} lang={lang} />
  </section>;
}

export function ItemCrafter({ session, lang, selectedSlot }: { session: BuildSession; lang: Lang; selectedSlot?: string | null }) {
  const t = (key: Parameters<typeof craftT>[1]) => craftT(lang, key);
  const [catalog, setCatalog] = useState<CraftCatalog | null>(null);
  const [error, setError] = useState(false);
  // Follow slot inspection only until the user edits or creates a draft.
  const [itemClass, setItemClass] = useState('');
  const [baseId, setBaseId] = useState('');
  const [rarity, setRarity] = useState<'NORMAL' | 'MAGIC' | 'RARE'>('RARE');
  const [itemLevel, setItemLevel] = useState(85);
  const [quality, setQuality] = useState(0);
  const [created, setCreated] = useState<{ text: string; id: number } | null>(null);
  useEffect(() => {
    let cancelled = false;
    Promise.all([getBackend().then(backend => backend.itemCraftingCatalog()), loadTradeCatalog()])
      .then(([game, display]) => { if (!cancelled) setCatalog(craftingCatalog(game, display)); })
      .catch(() => { if (!cancelled) setError(true); });
    return () => { cancelled = true; };
  }, []);
  const classes = useMemo(() => [...new Set(catalog?.bases.map(base => base.itemClass) ?? [])], [catalog]);
  const currentClass = itemClass || (catalog ? defaultCraftClass(catalog, selectedSlot, [...session.items, ...session.flasks, ...session.limbs].find(item => item.slot === selectedSlot)?.text) : '');
  const bases = useMemo(() => catalog?.bases.filter(base => base.itemClass === currentClass) ?? [], [catalog, currentClass]);
  const base = bases.find(base => base.id === baseId) ?? bases[0];
  const rarities = base?.affix_limit === 1 ? ['NORMAL', 'MAGIC'] as const : ['NORMAL', 'MAGIC', 'RARE'] as const;
  const chosenRarity = rarity === 'RARE' && base?.affix_limit === 1 ? 'MAGIC' : rarity;
  const names = useLocalizedNames(bases.map(base => ({ kind: 'base' as const, name: base.name })), lang);
  const classNames = useLocalizedNames(classes.map(name => ({ kind: 'itemClass' as const, name: itemClassName(name) })), lang);
  const implicits = useLocalizedLines(base?.implicits ?? [], lang);
  const reset = () => { setItemClass(currentClass); setCreated(null); };
  if (error) return <p role="alert">{t('error')}</p>;
  if (!catalog) return <p role="status">{t('loading')}</p>;
  return <section className="item-crafter ui-card" aria-label={t('craft')}>
    <p>{t('hint')}</p>
    <div className="item-crafter-fields">
      <label>{t('itemClass')}<AppSelect ariaLabel={t('itemClass')} value={currentClass}
        options={classes.map((value, i) => ({ value, label: itemClassLabel(value, classNames[i], lang) }))}
        onChange={value => { reset(); setItemClass(value); setBaseId(''); }} /></label>
      <label>{t('base')}<AppSelect ariaLabel={t('base')} value={base?.id ?? ''}
        options={bases.map((base, i) => ({ value: base.id, label: itemNameLabel(base.name, craftBaseDisplayName(base, lang, names[i] ?? base.name), lang), hint: `${t('requirement')} ${base.level}` }))}
        onChange={value => { reset(); setBaseId(value); }} /></label>
      <label>{t('rarity')}<AppSelect ariaLabel={t('rarity')} value={chosenRarity}
        options={rarities.map(value => ({ value, label: t(value) }))}
        onChange={value => { reset(); setRarity(value as typeof rarity); }} /></label>
      <label>{t('itemLevel')}<AppInput aria-label={t('itemLevel')} type="number" min="1" max="100" value={itemLevel}
        onChange={event => { reset(); setItemLevel(Number(event.target.value)); }} /></label>
      <label>{t('quality')}<AppInput aria-label={t('quality')} type="number" min="0" max="20" value={quality}
        onChange={event => { reset(); setQuality(Number(event.target.value)); }} /></label>
    </div>
    {base && <div className="item-crafter-base"><p>{t('requirement')} {base.level}</p>{shownLines(base.implicits ?? [], implicits).map((line, i) => <p key={i}>{line}</p>)}</div>}
    {!!catalog.missingMods && <p className="trade-notice">{t('missing')} {catalog.missingMods}</p>}
    <AppButton className="trade-primary" disabled={session.busy || !base || !Number.isInteger(itemLevel) || itemLevel < 1 || itemLevel > 100
      || !Number.isInteger(quality) || quality < 0 || quality > 20}
      onClick={() => {
        setItemClass(currentClass);
        setCreated({ text: baseDraftText(base, chosenRarity, itemLevel, quality), id: (created?.id ?? 0) + 1 });
      }}>{t('create')}</AppButton>
    {created && base && <ItemDraftWorkbench key={created.id} initialText={created.text} base={base} catalog={catalog} session={session} lang={lang} />}
  </section>;
}
