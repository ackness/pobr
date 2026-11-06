import { renderToStaticMarkup } from 'react-dom/server';
import { expect, test } from 'vitest';
import type { EnemyPresets } from '../../api/enemyPresets';
import { encounterInheritedValues } from '../../lib/configEncounter';
import { EnemyTierSummary } from './EnemyTierSummary';

const table: EnemyPresets = { max_enemy_level: 85, tiers: [{
  id: 'Pinnacle', min_level: 82, elemental_resist_bonus: 50, chaos_resist_bonus: 0,
  armour_mult_pct: { base: 100, num: 1100, den: 22 }, evasion_mult_pct: { base: 100, num: 548, den: 22 },
  enemy_mods: [{ name: 'CurseEffectOnSelf', mod_type: 'MORE', value: -50, effective_only: true }],
}] };

test.each([
  ['en-US', 'Tier preset', '50% less', 'Only preset armour/evasion multipliers are shown.'],
  ['zh-CN', '档位预设', '总降 50%', '护甲与闪避仅列档位倍率'],
  ['zh-TW', '檔位預設', '總降 50%', '護甲與閃避僅列檔位倍率'],
] as const)('shows direct preset facts, explicit resistance and limits in %s', (lang, title, curse, limit) => {
  const inputs = { enemyFireResist: 0 };
  const inherited = encounterInheritedValues(table, 'pinnacle', 84, inputs);
  const html = renderToStaticMarkup(<EnemyTierSummary table={table} tier="pinnacle" inputs={inputs} inherited={inherited} lang={lang} />);
  expect(html).toContain(title);
  expect(html).toContain('>0%</span>');
  expect(html).toContain('>50%</span>');
  expect(html).toContain('>82</span>');
  expect(html).toContain('>84</span>');
  expect(html).toContain('>150%</span>');
  expect(html).toContain('>124.91%</span>');
  expect(html).toContain(curse);
  expect(html).toContain(limit);
  expect(html).not.toMatch(/pinnacle|CurseEffectOnSelf|armour_mult_pct/);
});

test('missing or custom presets do not manufacture tier parameters', () => {
  for (const [snapshot, tier] of [[null, 'pinnacle'], [table, 'custom']] as const)
    expect(renderToStaticMarkup(<EnemyTierSummary table={snapshot} tier={tier} inputs={{}} inherited={{}} lang="en-US" />)).toBe('');
});
