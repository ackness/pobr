import { useEffect, useMemo, useState } from 'react';
import type { ItemLineJson } from '../../api/types';
import { getBackend } from '../../api/backend';
import { itemLines, rarityOf, useLocalizedLines, useLocalizedNames } from '../../hooks/useLocalizedLines';
import { isContinuation } from '../../lib/continuedLines';
import { bindT, messages, originKindLabel, t, type Lang } from '../../lib/i18n';
import { itemAttributeType, itemDamageType, itemDisplaySections, itemModTagLabel, itemValueParts, type DisplayItemLine } from '../../lib/itemDisplay';
import { useUi } from '../shared/UiContext';
import { AppTooltip } from '../shared/controls';
import { Badge } from '../ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../ui/tabs';
import './itemDisplay.css';

/** Key asynchronous results by input as well as cancelling them when the selection changes. */
function useClassifiedLines(text: string): ItemLineJson[] | null {
  const [result, setResult] = useState<{ text: string; lines: ItemLineJson[] } | null>(null);
  useEffect(() => {
    let cancelled = false;
    getBackend().then(backend => backend.classifyItemLines(text)).then(lines => {
      if (!cancelled) setResult({ text, lines });
    }).catch(() => { if (!cancelled) setResult(null); });
    return () => { cancelled = true; };
  }, [text]);
  return result?.text === text && result.lines.length > 0 ? result.lines : null;
}

/** Classified base lines resolve by base identity; every other line keeps the text path. */
export function useItemLineTexts(lines: readonly ItemLineJson[], lang: Lang): string[] {
  const translated = useLocalizedLines(lines.map(line => line.kind === 'base' ? '' : line.text), lang);
  const bases = useLocalizedNames(lines.flatMap(line => line.kind === 'base' ? [{ kind: 'base' as const, name: line.text }] : []), lang);
  let base = 0;
  return lines.map((line, index) => line.kind === 'base' ? bases[base++] ?? line.text : translated[index] ?? line.text);
}

export function ItemDisplay({ text, lang, rawDetails = false }: { text: string; lang: Lang; rawDetails?: boolean }) {
  const classified = useClassifiedLines(text);
  const fallback = useMemo<ItemLineJson[]>(() => itemLines(text).map((line, index) => ({
    text: line, kind: index === 0 ? 'name' : 'struct',
  })), [text]);
  const lines = classified ?? fallback;
  const translated = useItemLineTexts(lines, lang);
  return <ItemDisplayContent text={text} lang={lang} lines={lines} translated={translated} rawDetails={rawDetails} />;
}

export function ItemValues({ text }: { text: string }) {
  return <>{itemValueParts(text).map((part, index) => part.numeric
    ? <strong className="item-value" key={index}>{part.text}</strong> : part.text)}</>;
}

/** All three layouts share one projection; no layout edits the imported item or invents missing data. */
export function ItemDisplayContent({ text, lang, lines, translated, rawDetails = false }: {
  text: string; lang: Lang; lines: readonly ItemLineJson[]; translated: readonly string[]; rawDetails?: boolean;
}) {
  const { displayMode: style } = useUi();
  // A line a multi-line description translated into the line above shows no row.
  const shown = lines.flatMap((line, index) => isContinuation(line.text, translated[index]) ? [] : [index]);
  const model = itemDisplaySections(shown.map(index => lines[index]), shown.map(index => translated[index]));
  const tt = bindT(lang);
  const rarity = rarityOf(text);
  const fixedAffixes = rarity === 'unique' || rarity === 'relic';
  const affixOf = (line: DisplayItemLine) => line.kind === 'explicit' && !fixedAffixes ? line.affix : undefined;
  const tierOf = (line: DisplayItemLine) => line.kind === 'explicit' && !fixedAffixes ? line.tier : undefined;
  const prefixes = model.explicit.filter(line => affixOf(line) === 'prefix');
  const suffixes = model.explicit.filter(line => affixOf(line) === 'suffix');
  const hasAffixes = prefixes.length > 0 || suffixes.length > 0;
  const splitAffixes = style === 'grouped' && prefixes.length > 0 && suffixes.length > 0;
  const hasTiers = model.explicit.some(line => tierOf(line) != null);
  const modifiersLabel = tt('itemDisplay.modifiers');
  const affixLabels = { prefix: tt('itemDisplay.prefix'), suffix: tt('itemDisplay.suffix') };
  const effectLabels: Partial<Record<ItemLineJson['kind'], string>> = {
    implicit: tt('itemDisplay.implicit'), enchant: originKindLabel(lang, 'ItemEnchant'), rune: tt('itemDisplay.rune'),
    class_req: tt('itemDisplay.requirement'),
  };
  const sourceLabels = {
    crafted: tt('itemDisplay.crafted'), fractured: tt('itemDisplay.fractured'),
    desecrated: tt('trade.source.desecrated'), mutated: tt('itemDisplay.mutated'), custom: tt('common.custom'),
  };
  const { NORMAL, MAGIC, RARE } = messages(lang, 'itemCrafting');
  const rarityLabel = ({ normal: NORMAL, magic: MAGIC, rare: RARE, unique: tt('itemDisplay.unique'), relic: tt('itemDisplay.relic') } as Record<string, string>)[rarity];

  function rowAttributes(line: DisplayItemLine) {
    return {
      'data-affix': affixOf(line), 'data-kind': line.kind, 'data-damage': itemDamageType(line.original),
      'data-attribute': itemAttributeType(line.original), 'data-source': line.modifier_sources?.[0],
    };
  }

  function modifierText(line: DisplayItemLine) {
    return <>
      <ItemValues text={line.text} />
      {line.modifier_sources?.length || line.mod_tags?.length ? <span className="item-modifier-meta">
        {line.modifier_sources?.map(source =>
          <Badge key={source} variant="outline" data-modifier-source={source}>{sourceLabels[source]}</Badge>)}
        {Array.from(new Set(line.mod_tags), tag =>
          <Badge key={`tag:${tag}`} variant="secondary" data-mod-tag={tag}>{itemModTagLabel(tag, lang)}</Badge>)}
      </span> : null}
    </>;
  }

  function affixBadge(line: DisplayItemLine) {
    const affix = affixOf(line);
    return affix ? <span className="item-affix-side">{affixLabels[affix]}</span> : null;
  }

  function tierBadge(line: DisplayItemLine) {
    const tier = tierOf(line);
    if (tier == null) return null;
    const total = line.tier_total == null ? '' : tt('itemDisplay.tierTotal', { count: line.tier_total });
    return <AppTooltip content={tt('itemDisplay.tierTooltip') + total}>
      <Badge variant="outline" tabIndex={0} className={`item-affix-tier${tier === 1 ? ' text-primary border-primary/30' : ''}`}>T{tier}</Badge>
    </AppTooltip>;
  }

  function modifier(line: DisplayItemLine, showSide = true) {
    const effect = effectLabels[line.kind];
    const side = showSide && affixOf(line);
    return <div className="item-affix-row item-line" {...rowAttributes(line)} key={line.index}>
      {effect ? <span className="item-affix-kind">{effect}</span> : null}
      <span className="item-affix-text">{modifierText(line)}</span>
      {side || tierOf(line) != null ? <span className="item-affix-identity">
        {side && affixBadge(line)}{tierBadge(line)}
      </span> : null}
    </div>;
  }

  const extras = (model.effects.length > 0 || model.runes.length > 0) && <section className="item-extra-effects" aria-label={tt('itemDisplay.effects')}>
    {model.runes.length > 0 && <ul className="item-socketed-runes">{model.runes.map(line => <li key={line.index}>{line.text}</li>)}</ul>}
    {model.effects.length > 0 && <div className="item-effects-list">{model.effects.map(line => modifier(line))}</div>}
  </section>;

  return <article className={`item-display rarity-${rarity}`} data-item-display={style} aria-label={model.name || tt('itemDisplay.unnamedDetails')}>
    <header className="item-display-header">
      <div><h4 className="item-name">{model.name}</h4>{model.bases.map(line => <p className="item-display-base" key={line.index}>{line.text}</p>)}</div>
      <div className="item-display-badges">{rarityLabel && <Badge variant="outline" className="text-[var(--rarity-c)]">{rarityLabel}</Badge>}
        {model.states.map(line => <Badge key={line.index} variant="outline" className={/Corrupted$/i.test(line.original) ? 'text-destructive border-destructive/30' : ''}>{line.text}</Badge>)}
      </div>
    </header>
    {model.properties.length > 0 && <dl className="item-properties">{model.properties.map(property => <div key={property.index}>
      <dt>{property.label}</dt><dd>{property.value}</dd>
    </div>)}</dl>}
    {model.metadata.length > 0 && <div className="item-display-metadata">{model.metadata.map(line => <p key={line.index}>{line.text}</p>)}</div>}
    <Tabs defaultValue="details" className="item-display-tabs">
      <div className="item-display-tabbar"><TabsList variant="line" aria-label={tt('itemDisplay.view')}>
        <TabsTrigger value="details">{tt('itemDisplay.details')}</TabsTrigger>
        {!rawDetails && <TabsTrigger value="raw">{tt('common.originalText')}</TabsTrigger>}
      </TabsList>{hasTiers && <span className="item-tier-hint">{tt('itemDisplay.tierHint')}</span>}</div>
      <TabsContent value="details" className="item-display-body">
        {extras}
        {model.explicit.length > 0 && (style === 'compact' ? <table className="item-affix-table">
          <caption className="sr-only">{tt('itemDisplay.modifierCaption')}</caption>
          <thead><tr>
            <th scope="col">{tt('itemDisplay.modifierAndValues')}</th>
            {hasAffixes && <th className="item-affix-side-cell" scope="col">{tt('common.type')}</th>}
            {hasTiers && <th className="item-affix-tier-cell" scope="col">{tt('itemDisplay.tier')}</th>}
          </tr></thead>
          <tbody>{model.explicit.map(line => <tr className="item-line" key={line.index} {...rowAttributes(line)}>
            <td className="item-affix-text">{modifierText(line)}</td>
            {hasAffixes && <td className="item-affix-side-cell">{affixBadge(line)}</td>}
            {hasTiers && <td className="item-affix-tier-cell">{tierBadge(line)}</td>}
          </tr>)}</tbody>
        </table> : <section className="item-affix-group" aria-label={modifiersLabel}>
          {splitAffixes ? <>
            <div className="item-affix-columns">
              <div data-affix-group="prefix"><h5>{affixLabels.prefix}</h5>{prefixes.map(line => modifier(line, false))}</div>
              <div data-affix-group="suffix"><h5>{affixLabels.suffix}</h5>{suffixes.map(line => modifier(line, false))}</div>
            </div>
            {prefixes.length + suffixes.length < model.explicit.length && <div className="item-affix-remainder">
              {model.explicit.filter(line => !affixOf(line)).map(line => modifier(line))}
            </div>}
          </> : <>
            <h5>{modifiersLabel}</h5>
            <div className="item-affix-list">{model.explicit.map(line => modifier(line))}</div>
          </>}
        </section>)}
      </TabsContent>
      {!rawDetails && <TabsContent value="raw"><pre className="item-original-text">{text}</pre></TabsContent>}
    </Tabs>
    {rawDetails && <details><summary className="disclosure">{t(lang, 'compare.advanced')}</summary><pre className="item-original-text">{text}</pre></details>}
  </article>;
}
