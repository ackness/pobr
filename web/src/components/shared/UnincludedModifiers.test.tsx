import { expect, test } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { joinWrappedModifiers, unincludedGroups, UnincludedModifiers } from './UnincludedModifiers';
import { mergeModifierContinuations } from '../../lib/continuedLines';

test('unincluded modifiers group by actual passive source without exposing node IDs in the heading', () => {
  const groups = unincludedGroups(['Tree:123: first effect', 'Tree:456: second effect', 'Tree:123: third effect', 'Config: fourth effect'], 'zh-CN');
  expect(groups).toHaveLength(3);
  expect(groups[0]).toMatchObject({ node: 123, lines: ['first effect', 'third effect'] });
  const html = renderToStaticMarkup(<UnincludedModifiers lines={['Tree:123: first effect']} lang="zh-CN" />);
  expect(html).toContain('<h5>天赋树（缺少名称）</h5>');
  expect(html).toContain('暂无译文，显示原文');
  expect(html).toContain('<pre>Tree:123: first effect</pre>');
});

test('a complete wrapped rune description becomes one modifier before translation', () => {
  const lines = ['Bonded: 5% chance when collecting an Elemental Infusion to gain an', 'Bonded: additional Elemental Infusion of the same type'];
  const merged = joinWrappedModifiers(lines);
  expect(merged).toEqual(['Bonded: 5% chance when collecting an Elemental Infusion to gain an\nadditional Elemental Infusion of the same type']);
  expect(unincludedGroups(lines, 'en-US')[0].lines).toEqual(merged);
  expect(lines).toHaveLength(2);
  expect(joinWrappedModifiers(['Different modifier', lines[1]])).toEqual(['Different modifier', lines[1]]);
  // The generic merger leaves a second Bonded prefix separate; replacing this helper changes output.
  expect(mergeModifierContinuations(lines)).toEqual(lines);
  expect(mergeModifierContinuations(lines)).not.toEqual(merged);
});

test('item provenance comes from an exact imported modifier line match', () => {
  const groups = unincludedGroups(['A unique modifier', 'Other modifier'], 'en-US', [{ slot: 'ring1', text: 'Rarity: UNIQUE\nExample Ring\nGold Ring\nA unique modifier' }]);
  expect(groups[0].item?.slot).toBe('ring1');
  expect(groups[1].item).toBeUndefined();
  const ambiguous = unincludedGroups(['A unique modifier'], 'en-US', [
    { slot: 'ring1', text: 'Rarity: NORMAL\nRing\nA unique modifier' },
    { slot: 'ring2', text: 'Rarity: NORMAL\nRing\nA unique modifier' },
  ]);
  expect(ambiguous[0].item).toBeUndefined();
});

test.each([
  ['en-US', 'Original modifier text'],
  ['zh-CN', '词缀原文'],
  ['zh-TW', '詞綴原文'],
] as const)('diagnostic source lists have one native original-text disclosure in %s', (lang, label) => {
  const lines = ['Tree:123: First unknown effect', 'Config: Another unknown effect', 'Tree:123: Wrapped\neffect'];
  const html = renderToStaticMarkup(<UnincludedModifiers lines={lines} lang={lang} />);
  // The source and modifier lists contain no extra disclosure for each row.
  expect(html.match(/<details\b/g)).toHaveLength(1);
  expect(html.match(/<summary\b/g)).toHaveLength(1);
  expect(html).toContain(`<details class="unincluded-raw"><summary class="disclosure">${label}</summary>`);
  expect(html).not.toContain('class="modifier-raw"');
  expect(html).toContain('<ul class="unincluded-sources">');
  expect(html).toContain('<ul class="unincluded-lines">');
  expect(html).toContain(`<pre>${lines.join('\n')}</pre>`);
  expect(html).not.toMatch(/<summary[^>]*(?:tabindex|aria-hidden|disabled)/);
});
