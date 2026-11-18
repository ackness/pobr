import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it, vi } from 'vitest';
import type { ItemLineJson } from '../../api/types';
import { UiProvider } from '../shared/UiContext';
import { ItemDisplayContent, useItemLineTexts } from './ItemDisplay';

const hooks = vi.hoisted(() => ({
  lines: vi.fn((lines: string[]) => lines.map(line => line && `text:${line}`)),
  names: vi.fn((names: readonly { name: string }[]) => names.map(name => `base:${name.name}`)),
}));
vi.mock('../../hooks/useLocalizedLines', async original => ({
  ...await original<typeof import('../../hooks/useLocalizedLines')>(),
  useLocalizedLines: hooks.lines, useLocalizedNames: hooks.names,
}));

const lines: ItemLineJson[] = [
  { kind: 'name', text: 'Woe Rain' }, { kind: 'base', text: 'Seeker Bow' },
  { kind: 'struct', text: 'Item Level: 81' },
  { kind: 'rune', text: 'Bow Attacks fire an additional Arrow' },
  { kind: 'explicit', text: 'Adds 12 to 228 Lightning Damage', affix: 'prefix', tier: 1, tier_total: 9, mod_tags: ['lightning', 'damage'] },
  { kind: 'explicit', text: 'Unrecognised <effect> & retained text' },
  { kind: 'explicit', text: '14% increased Attack Speed', affix: 'suffix', tier: 2, tier_total: 5 },
];
it.each(['grouped', 'compact', 'game'] as const)('%s keeps explicit modifiers in source order with verified row labels', style => {
  const html = renderToStaticMarkup(<UiProvider lang="en-US" displayMode={style}>
    <ItemDisplayContent text="Rarity: RARE\nWoe Rain\nSeeker Bow" lang="en-US" lines={lines} translated={lines.map(line => line.text)} />
  </UiProvider>);
  expect(html).toContain(`data-item-display="${style}"`);
  expect(html).toContain('Bow Attacks fire an additional Arrow');
  expect(html).toContain('data-affix="prefix"');
  expect(html).toContain('data-affix="suffix"');
  expect(html).toContain('>Prefix<');
  expect(html).toContain('>Suffix<');
  expect(html).toContain('>T1<');
  expect(html).toContain('>T2<');
  const unknown = html.indexOf('Unrecognised &lt;effect&gt; &amp; retained text');
  expect(unknown).toBeGreaterThan(html.indexOf('Lightning Damage'));
  expect(unknown).toBeLessThan(html.indexOf('increased Attack Speed'));
  expect(html).not.toMatch(/Prefixes|Suffixes|Unclassified|Tier not identified/);
  expect(html.match(/data-mod-tag=/g)).toHaveLength(2);
  expect(html).toContain('<strong class="item-value">14%</strong>');
  expect(html).toContain('Original text');
  expect(html.includes('<table')).toBe(style === 'compact');
});

it.each(['UNIQUE', 'RELIC'] as const)('%s modifiers omit affix side and tier while retaining known tags', rarity => {
  const fixed = lines.filter(line => line.kind !== 'explicit').concat({
    kind: 'explicit', text: 'Special fixed effect', affix: 'prefix', tier: 1, mod_tags: ['damage'],
  });
  const html = renderToStaticMarkup(<UiProvider lang="en-US" displayMode="compact">
    <ItemDisplayContent text={`Rarity: ${rarity}\nFixed item`} lang="en-US" lines={fixed} translated={fixed.map(line => line.text)} />
  </UiProvider>);
  expect(html).toContain('Special fixed effect');
  expect(html).toContain('data-mod-tag="damage"');
  expect(html).not.toMatch(/Unclassified|Tier not identified|data-affix=|>Type<|>Tier<|>T1</);
});

it('only classified base lines resolve by base identity; equal text elsewhere stays a text line', () => {
  const jewel: ItemLineJson[] = [{ kind: 'name', text: 'Sapphire' }, { kind: 'base', text: 'Sapphire' },
    { kind: 'explicit', text: 'Sapphire' }];
  function Probe() { return <span>{useItemLineTexts(jewel, 'zh-CN').join('|')}</span>; }
  expect(renderToStaticMarkup(<Probe />)).toContain('text:Sapphire|base:Sapphire|text:Sapphire');
  expect(hooks.names).toHaveBeenLastCalledWith([{ kind: 'base', name: 'Sapphire' }], 'zh-CN');
  expect(hooks.lines).toHaveBeenLastCalledWith(['Sapphire', '', 'Sapphire'], 'zh-CN');
});

it('a description translated as one line shows no row for the lines it covers', () => {
  const unique: ItemLineJson[] = [
    { kind: 'name', text: 'Unique item' },
    { kind: 'explicit', text: 'Energy Shield Recharge starts after spending a total of' },
    { kind: 'explicit', text: '2000 Mana, no more than once every 2 seconds' },
    { kind: 'explicit', text: '+50 to maximum Life' },
  ];
  const translated = ['传奇物品', '总共消耗 2000 点魔力后，能量护盾充能开始，每 2 秒最多触发一次', '', '+50 生命上限'];
  const html = renderToStaticMarkup(<UiProvider lang="zh-CN" displayMode="grouped">
    <ItemDisplayContent text="Rarity: UNIQUE\nUnique item" lang="zh-CN" lines={unique} translated={translated} />
  </UiProvider>);
  expect(html.match(/class="item-affix-row/g)).toHaveLength(2);
  expect(html).toContain('能量护盾充能开始');
  expect(html).toContain('生命上限');
});

it('comparison cards keep original text in collapsed advanced details', () => {
  const html = renderToStaticMarkup(<UiProvider lang="en-US"><ItemDisplayContent rawDetails text="Unique ID: saved-id" lang="en-US" lines={lines} translated={lines.map(line => line.text)} /></UiProvider>);
  expect(html).toContain('<details><summary class="disclosure">Advanced details (original text)</summary>');
  expect(html).not.toContain('<details open');
  expect(html).toContain('Unique ID: saved-id');
  expect(html).not.toContain('value="raw"');
});

it.each(['grouped', 'compact', 'game'] as const)('%s shows known crafting and locked sources without guessing missing ones', style => {
  const sourced: ItemLineJson[] = [lines[0], lines[1],
    { kind: 'explicit', text: '+74 to Evasion Rating', modifier_sources: ['fractured'] },
    { kind: 'explicit', text: '+25 to maximum Energy Shield', modifier_sources: ['crafted'] },
    { kind: 'explicit', text: '+43% to Cold Resistance', affix: 'suffix', tier: 1, mod_tags: ['cold', 'damage', 'future_mod_tag'] },
    { kind: 'explicit', text: 'Unknown effect' },
  ];
  const html = renderToStaticMarkup(<UiProvider lang="zh-CN" displayMode={style}>
    <ItemDisplayContent text="Rarity: RARE\nItem\nBase" lang="zh-CN" lines={sourced} translated={sourced.map(line => line.text)} />
  </UiProvider>);
  expect(html).toContain('破裂 · 锁定');
  expect(html).toContain('data-modifier-source="crafted"');
  expect(html).toContain('data-attribute="cold"');
  expect(html).toContain('data-mod-tag="cold">冰霜');
  expect(html).toContain('data-mod-tag="damage">伤害');
  expect(html).toContain('data-mod-tag="future_mod_tag">Future mod tag');
  expect(html.match(/data-modifier-source=/g)).toHaveLength(2);
});
