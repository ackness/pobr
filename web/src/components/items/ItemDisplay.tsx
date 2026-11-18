import { useEffect, useMemo, useState } from 'react';
import type { ItemLineJson } from '../../api/types';
import { getBackend } from '../../api/backend';
import { itemLines, rarityOf, useLocalizedLines, useLocalizedNames } from '../../hooks/useLocalizedLines';
import { isContinuation } from '../../lib/continuedLines';
import { t, type Lang } from '../../lib/i18n';
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
  const w = (en: string, cn: string, tw = cn) => lang === 'en-US' ? en : lang === 'zh-TW' ? tw : cn;
  const rarity = rarityOf(text);
  const fixedAffixes = rarity === 'unique' || rarity === 'relic';
  const affixOf = (line: DisplayItemLine) => line.kind === 'explicit' && !fixedAffixes ? line.affix : undefined;
  const tierOf = (line: DisplayItemLine) => line.kind === 'explicit' && !fixedAffixes ? line.tier : undefined;
  const hasAffixes = model.explicit.some(affixOf);
  const hasTiers = model.explicit.some(line => tierOf(line) != null);
  const modifiersLabel = w('Modifiers', '词缀', '詞綴');
  const affixLabels = { prefix: w('Prefix', '前缀', '前綴'), suffix: w('Suffix', '后缀', '後綴') };
  const effectLabels: Partial<Record<ItemLineJson['kind'], string>> = {
    implicit: w('Implicit', '固有', '固定'), enchant: w('Enchant', '附魔'), rune: w('Rune', '符文'),
    class_req: w('Requirement', '需求'),
  };
  const sourceLabels = {
    crafted: w('Crafted', '工艺', '工藝'), fractured: w('Fractured · locked', '破裂 · 锁定', '破裂 · 鎖定'),
    desecrated: w('Desecrated', '亵渎', '褻瀆'), mutated: w('Mutated', '异化', '異化'), custom: w('Custom', '自定义', '自訂'),
  };
  const rarityLabel = ({ normal: w('Normal', '普通'), magic: w('Magic', '魔法'), rare: w('Rare', '稀有'), unique: w('Unique', '传奇', '傳奇'), relic: w('Relic', '遗珍', '遺珍') } as Record<string, string>)[rarity];

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
    const total = line.tier_total == null ? '' : w(` · ${line.tier_total} tiers for this base`, ` · 该基底共 ${line.tier_total} 档`, ` · 此基底共 ${line.tier_total} 階`);
    return <AppTooltip content={w('T1 is the highest tier for this base.', 'T1 为该基底同类词缀的最高档。', 'T1 為此基底同類詞綴的最高階。') + total}>
      <Badge variant="outline" tabIndex={0} className={`item-affix-tier${tier === 1 ? ' text-primary border-primary/30' : ''}`}>T{tier}</Badge>
    </AppTooltip>;
  }

  function modifier(line: DisplayItemLine) {
    const effect = effectLabels[line.kind];
    return <div className="item-affix-row item-line" {...rowAttributes(line)} key={line.index}>
      {effect ? <span className="item-affix-kind">{effect}</span> : null}
      <span className="item-affix-text">{modifierText(line)}</span>
      {affixOf(line) || tierOf(line) != null ? <span className="item-affix-identity">
        {affixBadge(line)}{tierBadge(line)}
      </span> : null}
    </div>;
  }

  const extras = (model.effects.length > 0 || model.runes.length > 0) && <section className="item-extra-effects" aria-label={w('Additional effects', '附加效果')}>
    <h5>{w('Additional effects', '附加效果')}</h5>
    {model.runes.length > 0 && <ul className="item-socketed-runes">{model.runes.map(line => <li key={line.index}>{line.text}</li>)}</ul>}
    <div className="item-effects-list">{model.effects.map(modifier)}</div>
  </section>;

  return <article className={`item-display rarity-${rarity}`} data-item-display={style} aria-label={model.name || w('Equipment details', '装备详情', '裝備詳情')}>
    <header className="item-display-header">
      <div><h4 className="item-name">{model.name}</h4>{model.bases.map(line => <p className="item-display-base" key={line.index}>{line.text}</p>)}</div>
      <div className="item-display-badges">{rarityLabel && <Badge variant="outline" className="text-[var(--rarity-c)]">{rarityLabel}</Badge>}
        {model.states.map(line => <Badge key={line.index} variant="outline" className={/^Corrupted$/i.test(line.original) ? 'text-destructive border-destructive/30' : ''}>{line.text}</Badge>)}
      </div>
    </header>
    {model.properties.length > 0 && <dl className="item-properties">{model.properties.map(property => <div key={property.index}>
      <dt>{property.label}</dt><dd>{property.value}</dd>
    </div>)}</dl>}
    {model.metadata.length > 0 && <div className="item-display-metadata">{model.metadata.map(line => <p key={line.index}>{line.text}</p>)}</div>}
    <Tabs defaultValue="details" className="item-display-tabs">
      <div className="item-display-tabbar"><TabsList variant="line" aria-label={w('Equipment view', '装备查看方式', '裝備檢視方式')}>
        <TabsTrigger value="details">{w('Details', '装备详情', '裝備詳情')}</TabsTrigger>
        {!rawDetails && <TabsTrigger value="raw">{w('Original text', '原始文本', '原始文字')}</TabsTrigger>}
      </TabsList>{hasTiers && <span className="item-tier-hint">{w('T1 is the highest tier', 'T1 为最高档', 'T1 為最高階')}</span>}</div>
      <TabsContent value="details" className="item-display-body">
        {style === 'game' && extras}
        {model.explicit.length > 0 && (style === 'compact' ? <table className="item-affix-table">
          <caption className="sr-only">{w('Equipment modifiers', '装备词缀', '裝備詞綴')}</caption>
          <thead><tr>
            <th scope="col">{w('Modifier and values', '词缀与数值', '詞綴與數值')}</th>
            {hasAffixes && <th className="item-affix-side-cell" scope="col">{w('Type', '类型', '類型')}</th>}
            {hasTiers && <th className="item-affix-tier-cell" scope="col">{w('Tier', '档位', '階級')}</th>}
          </tr></thead>
          <tbody>{model.explicit.map(line => <tr className="item-line" key={line.index} {...rowAttributes(line)}>
            <td className="item-affix-text">{modifierText(line)}</td>
            {hasAffixes && <td className="item-affix-side-cell">{affixBadge(line)}</td>}
            {hasTiers && <td className="item-affix-tier-cell">{tierBadge(line)}</td>}
          </tr>)}</tbody>
        </table> : <section className="item-affix-group" aria-label={modifiersLabel}>
          <h5>{modifiersLabel}</h5>{model.explicit.map(modifier)}
        </section>)}
        {style !== 'game' && extras}
      </TabsContent>
      {!rawDetails && <TabsContent value="raw"><pre className="item-original-text">{text}</pre></TabsContent>}
    </Tabs>
    {rawDetails && <details><summary className="disclosure">{t(lang, 'compare.advanced')}</summary><pre className="item-original-text">{text}</pre></details>}
  </article>;
}
