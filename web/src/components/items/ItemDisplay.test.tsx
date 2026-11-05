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
  { kind: 'explicit', text: 'Adds 12 to 228 Lightning Damage', affix: 'prefix', tier: 1, tier_total: 9 },
  { kind: 'explicit', text: '14% increased Attack Speed', affix: 'suffix', tier: 2, tier_total: 5 },
  { kind: 'explicit', text: 'Unrecognised <effect> & retained text' },
];
it.each(['grouped', 'compact', 'game'] as const)('%s retains all modifiers, identifies sources and highlights values', style => {
  const html = renderToStaticMarkup(<UiProvider lang="en-US" displayMode={style}>
    <ItemDisplayContent text="Rarity: RARE\nWoe Rain\nSeeker Bow" lang="en-US" lines={lines} translated={lines.map(line => line.text)} />
  </UiProvider>);
  expect(html).toContain(`data-item-display="${style}"`);
  expect(html).toContain('Bow Attacks fire an additional Arrow');
  expect(html).toContain('data-affix="prefix"');
  expect(html).toContain('data-affix="suffix"');
  expect(html).toContain('Unrecognised &lt;effect&gt; &amp; retained text');
  expect(html).toContain('Unclassified');
  expect(html).toContain('<strong class="item-value">14%</strong>');
  expect(html).toContain('Original text');
  expect(html.includes('<table')).toBe(style === 'compact');
});

it('unique modifiers have no inferred prefix, suffix or missing-tier warning', () => {
  const unique = lines.filter(line => line.kind !== 'explicit').concat({ kind: 'explicit', text: 'Special unique effect' });
  const html = renderToStaticMarkup(<UiProvider lang="en-US" displayMode="compact">
    <ItemDisplayContent text="Rarity: UNIQUE\nUnique item" lang="en-US" lines={unique} translated={unique.map(line => line.text)} />
  </UiProvider>);
  expect(html).toContain('Special unique effect');
  expect(html).not.toContain('Unclassified');
  expect(html).not.toContain('Tier not identified');
  expect(html).not.toContain('data-affix=');
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
