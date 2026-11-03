import { renderToStaticMarkup } from 'react-dom/server';
import { expect, test } from 'vitest';
import type { Breakdown, CalculateBuildResponse } from '../../api/types';
import { BreakdownTable, CalcsPanel } from './CalcsPanel';
import type { BuildSession } from '../../hooks/useBuildSession';
import { UiProvider } from '../shared/UiContext';
import { TooltipProvider } from '../ui/tooltip';
import details from '../../fixtures/stat_details.json';

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
