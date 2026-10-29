import { describe, expect, it } from 'vitest';
import type { ConfigOption } from '../../../api/types';
import { configApplicability } from '../../../lib/configVisibility';
import { SCENARIOS, activeScenario, hasVisibilityRules, partitionRelevant } from './scenario';

const option = (variable: string, visibility?: ConfigOption['visibility']): ConfigOption => ({ var: variable, input_type: 'check', visibility });

describe('scenario presets', () => {
  it('offers the four enemy tiers the engine accepts, easiest first', () => {
    expect(SCENARIOS.map(scenario => scenario.tier)).toEqual(['none', 'boss', 'pinnacle', 'uber']);
  });

  it('marks a build with no tier of its own as custom', () => {
    expect(activeScenario('boss')).toBe('boss');
    expect(activeScenario('none')).toBe('none');
    expect(activeScenario(undefined)).toBe('custom');
  });
});

describe('partitionRelevant', () => {
  const context = { main_skill: 'FrostBombPlayer', skill_types: [], predicates: { if_cond: { A: true, B: false, C: null }, if_flag: { F: true } } };
  const options = [
    option('plain'), option('empty', {}), option('confirmed', { if_cond: ['A'] }), option('refuted', { if_cond: ['B'] }),
    option('unknown', { if_cond: ['C'] }), option('both', { if_cond: ['A'], if_flag: ['F'] }), option('half', { if_cond: ['A'], if_flag: ['G'] }),
    option('either', { if_cond: ['B', 'A'] }),
  ];
  const split = partitionRelevant(options, candidate => configApplicability(candidate, context, {}));

  it('promotes only options whose rules the calculation confirmed', () => {
    expect(split.relevant.map(candidate => candidate.var)).toEqual(['confirmed', 'both', 'either']);
  });

  it('never singles out an option without a rule, or one that cannot be evaluated', () => {
    expect(split.rest.map(candidate => candidate.var)).toEqual(['plain', 'empty', 'refuted', 'unknown', 'half']);
  });

  it('promotes nothing before the calculation has produced a context', () => {
    expect(partitionRelevant(options, candidate => configApplicability(candidate, null, {})).relevant).toEqual([]);
  });

  it('treats an empty rule list as no rule', () => {
    expect(hasVisibilityRules(option('x', { if_cond: [] }))).toBe(false);
    expect(hasVisibilityRules(option('y', { if_cond: ['A'] }))).toBe(true);
  });
});
