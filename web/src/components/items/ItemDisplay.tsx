import { useEffect, useMemo, useState } from 'react';
import type { ItemLineJson } from '../../api/types';
import { getBackend } from '../../api/backend';
import { itemLines, rarityOf, useLocalizedLines } from '../../hooks/useLocalizedLines';
import type { Lang } from '../../lib/i18n';
import { itemDamageType, itemDisplaySections, itemValueParts, type DisplayItemLine } from '../../lib/itemDisplay';
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

export function ItemDisplay({ text, lang }: { text: string; lang: Lang }) {
  const classified = useClassifiedLines(text);
  const fallback = useMemo<ItemLineJson[]>(() => itemLines(text).map((line, index) => ({
    text: line, kind: index === 0 ? 'name' : 'struct',
  })), [text]);
  const lines = classified ?? fallback;
  const translated = useLocalizedLines(lines.map(line => line.text), lang);
  return <ItemDisplayContent text={text} lang={lang} lines={lines} translated={translated} />;
}

export function ItemValues({ text }: { text: string }) {
  return <>{itemValueParts(text).map((part, index) => part.numeric
    ? <strong className="item-value" key={index}>{part.text}</strong> : part.text)}</>;
}

/** All three layouts share one projection; no layout edits the imported item or invents missing data. */
export function ItemDisplayContent({ text, lang, lines, translated }: {
  text: string; lang: Lang; lines: readonly ItemLineJson[]; translated: readonly string[];
}) {
  const { itemDisplay: style } = useUi();
  const model = itemDisplaySections(lines, translated);
  const w = (en: string, cn: string, tw = cn) => lang === 'en-US' ? en : lang === 'zh-TW' ? tw : cn;
  const rarity = rarityOf(text);
  const fixedAffixes = rarity === 'unique' || rarity === 'relic';
  const hasTiers = !fixedAffixes && model.explicit.some(line => line.tier != null);
  const hasUnknownTiers = hasTiers && model.explicit.some(line => line.tier == null);
  const prefixLabel = w('Prefixes', '前缀', '前綴');
  const suffixLabel = w('Suffixes', '后缀', '後綴');
  const otherLabel = fixedAffixes ? w('Modifiers', '词条', '詞綴') : w('Unclassified modifiers', '未分类词条', '未分類詞綴');
  const labels = {
    prefix: w('Prefix', '前缀', '前綴'), suffix: w('Suffix', '后缀', '後綴'),
    implicit: w('Implicit', '固有', '固定'), enchant: w('Enchant', '附魔'), rune: w('Rune', '符文'),
    class_req: w('Requirement', '需求'),
    explicit: fixedAffixes ? w('Modifier', '词条', '詞綴') : w('Unclassified', '未分类', '未分類'),
  };
  const rarityLabel = ({ normal: w('Normal', '普通'), magic: w('Magic', '魔法'), rare: w('Rare', '稀有'), unique: w('Unique', '传奇', '傳奇'), relic: w('Relic', '遗珍', '遺珍') } as Record<string, string>)[rarity];

  function tier(line: DisplayItemLine) {
    if (fixedAffixes || !hasTiers) return null;
    if (line.tier == null) return <span className="item-affix-tier item-affix-tier-unknown" aria-label={w('Tier not identified', '档位未识别', '階級未識別')}>—</span>;
    const total = line.tier_total == null ? '' : w(` · ${line.tier_total} tiers for this base`, ` · 该基底共 ${line.tier_total} 档`, ` · 此基底共 ${line.tier_total} 階`);
    return <AppTooltip content={w('T1 is the highest tier for this base.', 'T1 为该基底同类词缀的最高档。', 'T1 為此基底同類詞綴的最高階。') + total}>
      <Badge variant="outline" tabIndex={0} className={`item-affix-tier${line.tier === 1 ? ' text-primary border-primary/30' : ''}`}>T{line.tier}</Badge>
    </AppTooltip>;
  }

  function modifier(line: DisplayItemLine, showKind = false) {
    const kind = line.kind === 'explicit' && !fixedAffixes ? line.affix ?? 'explicit' : line.kind;
    return <div className="item-affix-row item-line" data-affix={line.affix} data-kind={line.kind} data-damage={itemDamageType(line.original)} key={line.index}>
      {showKind && <span className="item-affix-kind">{kind === 'explicit' && !fixedAffixes ? w('Unknown', '未分类', '未分類') : labels[kind as keyof typeof labels]}</span>}
      <span className="item-affix-text"><ItemValues text={line.text} /></span>
      {line.kind === 'explicit' && tier(line)}
    </div>;
  }

  function group(label: string, groupLines: DisplayItemLine[]) {
    return groupLines.length > 0 && <section className="item-affix-group" aria-label={label}>
      <h5>{label}</h5>{groupLines.map(line => modifier(line, style === 'game'))}
    </section>;
  }

  const extras = (model.effects.length > 0 || model.runes.length > 0) && <section className="item-extra-effects" aria-label={w('Additional effects', '附加效果')}>
    <h5>{w('Additional effects', '附加效果')}</h5>
    {model.runes.length > 0 && <ul className="item-socketed-runes">{model.runes.map(line => <li key={line.index}>{line.text}</li>)}</ul>}
    <div className="item-effects-list">{model.effects.map(line => modifier(line, true))}</div>
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
        <TabsTrigger value="raw">{w('Original text', '原始文本', '原始文字')}</TabsTrigger>
      </TabsList>{hasTiers && <span className="item-tier-hint">{w('T1 is the highest tier', 'T1 为最高档', 'T1 為最高階')}</span>}</div>
      <TabsContent value="details" className="item-display-body">
        {style === 'game' && extras}
        {style === 'compact' && model.explicit.length > 0 ? <table className="item-affix-table">
          <caption className="sr-only">{w('Equipment modifiers', '装备词缀', '裝備詞綴')}</caption>
          <thead><tr><th scope="col">{w('Type', '类型', '類型')}</th><th scope="col">{w('Modifier and values', '词条与数值', '詞綴與數值')}</th>{hasTiers && <th scope="col">{w('Tier', '档位', '階級')}</th>}</tr></thead>
          <tbody>{model.explicit.map(line => <tr className="item-line" key={line.index} data-affix={line.affix} data-damage={itemDamageType(line.original)}>
            <td>{line.affix && !fixedAffixes ? <Badge variant={line.affix === 'prefix' ? 'secondary' : 'outline'}>{labels[line.affix]}</Badge>
              : <span className="item-unclassified">{labels.explicit}</span>}</td>
            <td className="item-affix-text"><ItemValues text={line.text} /></td>{hasTiers && <td>{tier(line)}</td>}
          </tr>)}</tbody>
        </table> : <div className="item-affix-groups">
          {fixedAffixes ? group(otherLabel, model.explicit) : <>
            {group(prefixLabel, model.prefixes)}{group(suffixLabel, model.suffixes)}
            {group(otherLabel, model.other)}
          </>}
        </div>}
        {style !== 'game' && extras}
        {hasUnknownTiers && <p className="item-display-hint">{w('— means the tier was not identified.', '— 表示档位未识别。', '— 表示階級未識別。')}</p>}
      </TabsContent>
      <TabsContent value="raw"><pre className="item-original-text">{text}</pre></TabsContent>
    </Tabs>
  </article>;
}
