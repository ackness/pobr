import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it } from 'vitest';
import type { ItemLineJson } from '../../api/types';
import { UiProvider } from '../shared/UiContext';
import { ItemDisplayContent } from './ItemDisplay';

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
