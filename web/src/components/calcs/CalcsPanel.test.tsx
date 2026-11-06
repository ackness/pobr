import { renderToStaticMarkup } from 'react-dom/server';
import { expect, test } from 'vitest';
import type { Breakdown, BreakdownMod, CalculateBuildResponse, DetailKind } from '../../api/types';
import { BreakdownTable, CalcsPanel, followedActorKey } from './CalcsPanel';
import type { BuildSession } from '../../hooks/useBuildSession';
import { UiProvider } from '../shared/UiContext';
import { TooltipProvider } from '../ui/tooltip';
import details from '../../fixtures/stat_details.json';
import { bindT, type Lang } from '../../lib/i18n';

const empty: Breakdown = { base_total: 0, inc_total: 0, more_total: 1, mods: [], formula: null };

test.each([[0, '∞'], [3, '3']] as const)('ailment details show the stack ceiling %s', (cap, displayed) => {
  const fixture = details.IgniteDPS as Breakdown;
  const breakdown = { ...fixture, ailment: { ...fixture.ailment!, max_stacks: cap } };
  const html = renderToStaticMarkup(<BreakdownTable breakdown={breakdown} lang="zh-CN" />);
  expect(html).toMatch(new RegExp(`class="ailment-maxStacks"><th[^>]*>[^<]*</th><td[^>]*>${displayed}</td>`));
});

test.each([[100, '∞'], [0, '0']] as const)('poison cap distinguishes an unlimited source from no poison (%s)', (chance, displayed) => {
  const cap: Breakdown = { ...empty, kind: 'derived', derived: {
    formula: 'MaximumAilmentStacks',
    inputs: [{ id: 'MaximumAilmentStacks', value: 0 }],
    context: [],
  } };
  const session = { calc: {
    stats: [
      { id: 'PoisonStacksMax', category: 'Ailment', value: 0 },
      { id: 'PoisonChance', category: 'Ailment', value: chance },
    ],
    breakdowns: { PoisonStacksMax: cap },
    breakdown_descriptors: { PoisonStacksMax: 'derived' },
    minions: [],
  }, socketGroups: [], calcParams: {} } as unknown as BuildSession;
  const html = renderToStaticMarkup(<UiProvider lang="en-US"><TooltipProvider>
    <CalcsPanel session={session} lang="en-US" />
  </TooltipProvider></UiProvider>);
  expect(html).toContain(`<div class="calcs-detail-value"><strong>${displayed}</strong>`);
  expect(html).toContain(`<td>${displayed}</td>`);
});

test('recalculation retains the metric list and settled detail instead of collapsing the page', () => {
  const settled = { stats: [{ id: 'Life', category: 'Resource', value: 1234 }],
    breakdowns: { Life: { ...empty, base_total: 1234 } }, breakdown_descriptors: { Life: 'steps' }, minions: [] } as unknown as CalculateBuildResponse;
  const session = { calc: null, busy: true, socketGroups: [], calcParams: {} } as unknown as BuildSession;
  const html = renderToStaticMarkup(<UiProvider lang="en-US"><TooltipProvider>
    <CalcsPanel session={session} settledCalc={settled} lang="en-US" />
  </TooltipProvider></UiProvider>);
  expect(html).toContain('class="calcs-explorer"');
  expect(html).toContain('aria-busy="true"');
  expect(html).toContain('1,234');
  expect(html).toContain('Base total: <strong>1,234</strong>');
  const failed = renderToStaticMarkup(<UiProvider lang="en-US"><TooltipProvider>
    <CalcsPanel session={{ ...session, calcError: 'Failed calculation' }} settledCalc={settled} lang="en-US" />
  </TooltipProvider></UiProvider>);
  expect(failed).toContain('Failed calculation');
  expect(failed).not.toContain('class="calcs-explorer"');
});

test('Calcs selects an available resource without eager skill overview or attribution', () => {
  const session = { calc: { stats: [{ id: 'Life', category: 'Resource', value: 100 }],
    breakdowns: {}, breakdown_descriptors: { Life: 'steps' }, minions: [] }, socketGroups: [], calcParams: {} } as unknown as BuildSession;
  const html = renderToStaticMarkup(<UiProvider lang="en-US"><TooltipProvider>
    <CalcsPanel session={session} lang="en-US" />
  </TooltipProvider></UiProvider>);
  expect(html).toContain('id="calcs-detail"');
  expect(html).toContain('aria-pressed="true"');
  expect(html).not.toContain('Choose a metric to inspect its calculation.');
  expect(html).toContain('aria-controls="calcs-detail"');
  expect(html).toContain('data-detail-kind="steps"');
  expect(html).toContain('Metrics &amp; details');
  expect(html).not.toContain('id="skill-metrics-heading"');
  expect(html).not.toContain('id="attribution-heading"');
});

test.each([
  ['en-US', 'Energy trigger rate / own use rate'],
  ['zh-CN', '能量触发频率 / 自身使用频率'],
  ['zh-TW', '能量觸發頻率 / 自身使用頻率'],
] as const)('Calcs renders the Energy multiplier and result in %s', (lang, label) => {
  const breakdown: Breakdown = { ...empty, formula: {
    steps: [
      { op: 'hit_dps', operand: 1000, result: 1000 },
      { op: 'energy_trigger', operand: 0.25, result: 250 },
    ],
    value: 250,
  } };
  const html = renderToStaticMarkup(<BreakdownTable breakdown={breakdown} lang={lang} />);
  expect(html).toContain(`<tr class="formula-step formula-energy_trigger"><th scope="row">${label}</th><td class="formula-operand">×0.25</td><td class="formula-result">250</td></tr>`);
  expect(html).toContain('class="formula-final"');
});

test.each([
  ['EnergyNeedsIncomingHits', 'Incoming blockable hits per second'],
  ['EnergyNeedsMinionDeaths', 'Minions killed per second'],
  ['EnergyNeedsSpellCasts', 'self-cast spell'],
  ['EnergyNeedsAilments', 'Ignite, Shock or Freeze'],
  ['EnergyNeedsCriticalHits', 'critically hit'],
  ['EnergyNeedsBlockChance', 'chance to block'],
])('Calcs explains the own-rate fallback for %s', (id, hint) => {
  const breakdown: Breakdown = { ...empty, kind: 'derived', derived: {
    formula: '0 * EnergyAtOwnRate',
    inputs: [{ id: 'EnergyAtOwnRate', value: 2 }],
    context: [{ id, value: 1 }],
  } };
  const html = renderToStaticMarkup(<BreakdownTable breakdown={breakdown} lang="en-US" />);
  expect(html).toContain('class="breakdown-note"');
  expect(html).toContain('own rate.');
  expect(html).toContain(hint);
  expect(html).toContain('Energy source unavailable');
  expect(html).not.toContain(id);
});

function panel(calc: Partial<CalculateBuildResponse>, lang: Lang = 'en-US', focus?: { id: string }) {
  const session = { calc: { minions: [], breakdowns: {}, ...calc }, socketGroups: [], calcParams: {} } as unknown as BuildSession;
  return renderToStaticMarkup(<UiProvider lang={lang}><TooltipProvider>
    <CalcsPanel session={session} lang={lang} focus={focus} />
  </TooltipProvider></UiProvider>);
}

test.each(['en-US', 'zh-CN', 'zh-TW'] as const)('Calcs shows honest empty states without a kind badge in %s', lang => {
  const tt = bindT(lang);
  for (const kind of ['steps', 'derived', 'none'] as DetailKind[]) {
    const html = panel({ stats: [{ id: 'Life', category: 'Resource', value: 0 }],
      breakdowns: { Life: empty }, breakdown_descriptors: { Life: kind } }, lang);
    expect(html).toContain(tt(kind === 'none' ? 'calcs.noBreakdown' : 'calcs.zeroNoRows'));
    expect(html).toContain('<div class="calcs-detail-value"><strong>0</strong></div>');
    expect(html).not.toContain('class="breakdown-heading"');
  }
  const unavailable = panel({ stats: [{ id: 'Life', category: 'Resource', value: 100 }], breakdowns: { Life: empty }, breakdown_descriptors: { Life: 'steps' } }, lang);
  expect(unavailable).toContain(tt('calcs.noBreakdown'));
});

test.each(['en-US', 'zh-CN', 'zh-TW'] as const)('a summoner build opens on the main minion in %s', lang => {
  const calc = { stats: [{ id: 'CombinedDPS', category: 'Offence', value: 0 }], main_skill: { skill_id: 'Wolf Pack' },
    breakdowns: { CombinedDPS: empty }, breakdown_descriptors: { CombinedDPS: 'steps' }, main_minion_index: 7,
    minions: [
      { index: 2, name: 'Decoy', minion_id: 'decoy', summoned_by: 'Decoy', stats: { TotalDPS: 0 }, breakdowns: {} },
      { index: 7, name: 'Wolf Companion', minion_id: 'wolf', summoned_by: 'Wolf Pack', stats: { TotalDPS: 5307.37 }, breakdowns: {} },
    ],
  } as unknown as CalculateBuildResponse;
  const html = panel(calc, lang);
  const tt = bindT(lang);
  const message = tt('calcs.minionDamage').replace('{skill}', lang === 'en-US' ? 'Wolf Pack' : tt('skills.untranslated').replace('{name}', 'Wolf Pack'));
  expect(html).toContain(tt('calcs.actorEyebrow').replace('{actor}', `${tt('minion.title')} · Wolf Companion #8`));
  expect(html).toContain('<div class="calcs-detail-value"><strong>5,307</strong></div>');
  // The player's zero is no longer what the page opens on, so the pointer to the minion isn't needed.
  expect(html).not.toContain(message);
  const player = panel({ ...calc, main_minion_index: undefined }, lang);
  expect(player).toContain(tt('calcs.actorEyebrow').replace('{actor}', tt('minion.player')));
  expect(player).not.toContain(message);
  // Arriving from the player's own zero opens on the player, with a pointer to the minion.
  const fromStat = panel(calc, lang, { id: 'CombinedDPS' });
  expect(fromStat).toContain(tt('calcs.actorEyebrow').replace('{actor}', tt('minion.player')));
  expect(fromStat).toContain(renderToStaticMarkup(<p className="breakdown-note">{message}</p>));
  expect(fromStat).toContain(tt('calcs.viewMinion').replace('{minion}', 'Wolf Companion'));
  expect(fromStat).toContain('<div class="calcs-detail-value"><strong>0</strong></div>');
  expect(panel({ ...calc, stats: [{ id: 'CombinedDPS', category: 'Offence', value: 5 }] }, lang, { id: 'CombinedDPS' })).not.toContain('class="calcs-minion-damage"');
  expect(panel({ ...calc, stats: [{ id: 'Life', category: 'Resource', value: 0 }] }, lang, { id: 'Life' })).not.toContain('class="calcs-minion-damage"');
});

test('derived sections show readable formulas and stat-specific input units', () => {
  const breakdown: Breakdown = { ...empty, kind: 'derived', derived: {
    formula: 'round(PrimaryMitigatedHit * PrimaryHitChance, 9)',
    inputs: [{ id: 'PrimaryMitigatedHit', value: 254121.8 }, { id: 'PrimaryHitChance', value: 1 }, { id: 'CritChance', value: 22.35 }],
    context: [{ id: 'FireResist', value: 20 }],
  } };
  const html = renderToStaticMarkup(<BreakdownTable breakdown={breakdown} lang="zh-CN" />);
  expect(html).toContain('<h4 class="breakdown-heading">计算公式</h4>');
  expect(html).toContain('减免后平均击中伤害 × 命中率');
  expect(html).not.toContain('round(');
  for (const value of ['100%', '22.4%', '20%']) expect(html).toContain(`<td>${value}</td>`);
});

test.each([[5000, 5000], [0, 5000], [5000, 0]])('zero DPS components with positive overall damage do not show the minion notice (%s / %s)', (combined, total) => {
  const html = panel({
    stats: [
      { id: 'IgniteDPS', category: 'Ailment', value: 0 },
      { id: 'CombinedDPS', category: 'Offence', value: combined },
      { id: 'TotalDPS', category: 'Offence', value: total },
    ],
    breakdowns: {}, breakdown_descriptors: { IgniteDPS: 'none' }, main_minion_index: 7,
    minions: [{ index: 7, name: 'Wolf Companion', minion_id: 'wolf', summoned_by: 'Wolf Pack', stats: { TotalDPS: 5307.37 }, breakdowns: {} }],
  } as unknown as CalculateBuildResponse, 'en-US', { id: 'IgniteDPS' });
  expect(html).toContain('<h3 id="calcs-detail-heading" tabindex="-1">Ignite DPS</h3>');
  expect(html).toContain('<div class="calcs-detail-value"><strong>0</strong></div>');
  expect(html).not.toContain('class="calcs-minion-damage"');
});

test.each([['Life', 1693.83, '1,694'], ['TotalDPS', 5307.37, '5,307'], ['CritChance', 22.35, '22.4%'], ['FireResist', 20, '20%']] as const)(
  'final %s result uses the header format', (stat, value, expected) => {
    const breakdown: Breakdown = { ...empty, actor: { kind: 'minion', index: 0 }, formula: {
      steps: [{ op: 'base', operand: value, result: value }], value,
    } };
    const html = renderToStaticMarkup(<BreakdownTable breakdown={breakdown} stat={stat} lang="en-US" />);
    expect(html).toContain(`<tr class="formula-final"><th scope="row">Result</th><td></td><td class="formula-result">${expected}</td></tr>`);
    expect(html).toContain('<h4 class="breakdown-heading">How it&#x27;s calculated</h4>');
  },
);

test('source groups share one totals note and show origin-specific fallback text', () => {
  const mods: BreakdownMod[] = ['CharacterBase', 'GameConstant', 'SupportGem', 'Campaign'].map(origin_kind => ({
    mod_type: 'BASE', value: 20, active: false, source_text: null, origin_kind, origin_id: null, slot: null,
  }));
  const breakdown: Breakdown = { ...empty, kind: 'sources', sources: ['Life', 'Mana'].map(name => ({ ...empty, name, more_precision: null, mods })) };
  const html = renderToStaticMarkup(<BreakdownTable breakdown={breakdown} lang="en-US" />);
  expect(html.match(/class="breakdown-note breakdown-totals-note"/g)).toHaveLength(1);
  expect(html).toContain('Modifiers that affect this value');
  for (const text of ['Base value from level and attributes', 'Game base value', 'Support gem effect', 'No modifier text (base value)']) {
    expect(html).toContain(`${text} <span class="mod-inactive-tag">`);
  }
  expect(html).not.toContain('Campaign resistance penalty');
});

test('modifiers fold by source with a count and separate stacking subtotals', () => {
  const mods: BreakdownMod[] = [
    { mod_type: 'BASE', value: 20, active: true, source_text: '20 to Life', origin_kind: 'PassiveNode', origin_id: 'node.1', slot: null },
    { mod_type: 'INC', value: 10, active: true, source_text: '10% increased Life', origin_kind: 'PassiveNode', origin_id: 'node.2', slot: null },
  ];
  const html = renderToStaticMarkup(<BreakdownTable breakdown={{ ...empty, mods }} modifierMeasure={{ stat: 'Life', unit: 'points' }} lang="en-US" />);
  expect(html).toContain('<details class="modifier-source-group"><summary><strong>Passive tree</strong> <span>(2)</span>');
  expect(html).toContain('Base 20');
  expect(html).toContain('Increased / Reduced 10%');
  expect(html).not.toContain('Base 30');
});

test('resource formula summary preserves operation order and reports missing conversion destination', () => {
  const breakdown: Breakdown = { ...empty, formula: { value: 78, steps: [
    { op: 'flat', operand: 100, result: 100 }, { op: 'converted', operand: 50, result: 50 },
    { op: 'gained', operand: 10, result: 60 }, { op: 'increased', operand: 30, result: 78 },
  ] } };
  const html = renderToStaticMarkup(<BreakdownTable breakdown={breakdown} stat="Life" lang="en-US" />);
  expect(html.replace(/<[^>]*>/g, '')).toContain('(((100 ×0.5) +10) ×1.3)');
  expect(html).toContain('78 points');
  expect(html).toContain('Conversion destination is not returned');
  expect(html).toContain('<details class="formula-steps"><summary>How it&#x27;s calculated</summary>');
});

test('resistance figures use returned total, cap and over-cap and show missing sources honestly', () => {
  const html = renderToStaticMarkup(<BreakdownTable breakdown={empty} stat="FireResist" lang="en-US"
    values={new Map([['FireResistTotal', 113], ['FireResistMax', 75], ['FireResistOverCap', 38]])} />);
  expect(html).toContain('<h4>Total</h4><strong class="mod-value">113%</strong>');
  expect(html).toContain('<h4>Cap</h4><strong class="mod-value">75%</strong>');
  expect(html).toContain('<h4>Over cap</h4><strong class="mod-value">38%</strong>');
  expect(html).toContain('does not provide the source');
  const missing = renderToStaticMarkup(<BreakdownTable breakdown={empty} stat="FireResist" lang="en-US" />);
  expect(missing).toContain('Not returned by this calculation');
  expect(missing).not.toContain('38%');
});

test('item names are shown as sources instead of being passed off as modifier text', () => {
  const html = renderToStaticMarkup(<BreakdownTable breakdown={{ ...empty, mods: [{ mod_type: 'BASE', value: 20, active: true,
    source_text: 'Mageblood', origin_kind: 'Item', origin_id: 'item.belt', slot: 'belt' }] }} lang="en-US" />);
  expect(html).toContain('Modifier text unavailable');
  expect(html).toContain('Item base · Mageblood');
  expect(html).not.toContain('<span>Mageblood</span>');
});

test('damage chain lists missing links and shows available final DPS', () => {
  const html = renderToStaticMarkup(<BreakdownTable breakdown={{ ...empty, formula: { value: 100, steps: [{ op: 'average_hit', operand: 100, result: 100 }] } }} stat="TotalDPS" lang="en-US" />);
  expect(html).toContain('From hit to DPS');
  expect(html).toContain('Unavailable links: After enemy mitigation, Uses per second, Trigger multiplier.');
  expect(html).toContain('100 DPS');
});


test('mixed enemy defence lines keep their values without displaying an invented subtotal', () => {
  const mods: BreakdownMod[] = [2000, 10].map(value => ({ mod_type: 'BASE', value, active: true,
    source_text: value === 2000 ? '2000 Armour' : '10% Physical Damage Reduction', origin_kind: 'EnemyConfig', origin_id: null, slot: null }));
  const html = renderToStaticMarkup(<BreakdownTable breakdown={{ ...empty, mods }} lang="en-US" />);
  expect(html).toContain('2,000');
  expect(html).toContain('10');
  expect(html).not.toContain('modifier-subtotals');
  expect(html).not.toContain('2,010');
});

test('the followed actor key changes across a failed calculation, so a later player skill leaves the minion', () => {
  const minion = { index: 8, minion_id: 'wolf', summoned_by: 'WolfPack', skill_id: 'Bite', dps: 5307 };
  const withMinion = { main_minion_index: 8, minions: [minion] } as unknown as CalculateBuildResponse;
  const playerSkill = { main_minion_index: undefined, minions: [minion] } as unknown as CalculateBuildResponse;
  const keys = [withMinion, null, playerSkill].map(followedActorKey);
  expect(keys[0]).not.toBe('player');
  expect(keys[1]).toBeNull();
  expect(keys[2]).toBe('player');
  // Each step differs from the one before, so the follow effect runs on recovery.
  expect(new Set(keys).size).toBe(3);
});
