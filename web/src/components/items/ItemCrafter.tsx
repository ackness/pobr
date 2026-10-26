import { AppButton, AppInput } from '../shared/controls';
import { useEffect, useMemo, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useLocalizedLines } from '../../hooks/useLocalizedLines';
import type { Lang } from '../../lib/i18n';
import { baseDraftText, craftingCatalog, type CraftCatalog } from '../../lib/itemCrafting';
import { craftT } from '../../lib/itemCraftingText';
import { loadTradeCatalog } from '../../lib/tradeOptimizer';
import { AppSelect } from '../shared/AppSelect';
import { ItemDraftWorkbench } from './ItemDraftWorkbench';
import './itemCrafter.css';

export function ItemCrafter({ session, lang }: { session: BuildSession; lang: Lang }) {
  const t = (key: Parameters<typeof craftT>[1]) => craftT(lang, key);
  const [catalog, setCatalog] = useState<CraftCatalog | null>(null);
  const [error, setError] = useState(false);
  const [itemClass, setItemClass] = useState('Ring');
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
  const bases = useMemo(() => catalog?.bases.filter(base => base.itemClass === itemClass) ?? [], [catalog, itemClass]);
  const base = bases.find(base => base.id === baseId) ?? bases[0];
  const rarities = base?.affix_limit === 1 ? ['NORMAL', 'MAGIC'] as const : ['NORMAL', 'MAGIC', 'RARE'] as const;
  const chosenRarity = rarity === 'RARE' && base?.affix_limit === 1 ? 'MAGIC' : rarity;
  const names = useLocalizedLines(bases.map(base => base.name), lang);
  const classNames = useLocalizedLines(classes, lang);
  const implicits = useLocalizedLines(base?.implicits ?? [], lang);
  const reset = () => setCreated(null);
  if (error) return <p role="alert">{t('error')}</p>;
  if (!catalog) return <p role="status">{t('loading')}</p>;
  return <section className="item-crafter ui-card" aria-label={t('craft')}>
    <p>{t('hint')}</p>
    <div className="item-crafter-fields">
      <label>{t('itemClass')}<AppSelect ariaLabel={t('itemClass')} value={itemClass}
        options={classes.map((value, i) => ({ value, label: classNames[i] ?? value }))}
        onChange={value => { reset(); setItemClass(value); setBaseId(''); }} /></label>
      <label>{t('base')}<AppSelect ariaLabel={t('base')} value={base?.id ?? ''}
        options={bases.map((base, i) => ({ value: base.id, label: names[i] ?? base.name, hint: `${t('requirement')} ${base.level}` }))}
        onChange={value => { reset(); setBaseId(value); }} /></label>
      <label>{t('rarity')}<AppSelect ariaLabel={t('rarity')} value={chosenRarity}
        options={rarities.map(value => ({ value, label: t(value) }))}
        onChange={value => { reset(); setRarity(value as typeof rarity); }} /></label>
      <label>{t('itemLevel')}<AppInput aria-label={t('itemLevel')} type="number" min="1" max="100" value={itemLevel}
        onChange={event => { reset(); setItemLevel(Number(event.target.value)); }} /></label>
      <label>{t('quality')}<AppInput aria-label={t('quality')} type="number" min="0" max="20" value={quality}
        onChange={event => { reset(); setQuality(Number(event.target.value)); }} /></label>
    </div>
    {base && <div className="item-crafter-base"><p>{t('requirement')} {base.level}</p>{implicits.map((line, i) => <p key={i}>{line}</p>)}</div>}
    {!!catalog.missingMods && <p className="trade-notice">{t('missing')} {catalog.missingMods}</p>}
    <AppButton className="trade-primary" disabled={session.busy || !base || !Number.isInteger(itemLevel) || itemLevel < 1 || itemLevel > 100
      || !Number.isInteger(quality) || quality < 0 || quality > 20}
      onClick={() => setCreated({ text: baseDraftText(base, chosenRarity, itemLevel, quality), id: (created?.id ?? 0) + 1 })}>{t('create')}</AppButton>
    {created && base && <ItemDraftWorkbench key={created.id} initialText={created.text} base={base} catalog={catalog} session={session} lang={lang} />}
  </section>;
}
