import { describe, expect, it } from 'vitest';
import type { ConfigOption } from '../../../api/types';
import { configApplicability } from '../../../lib/configVisibility';
import { SCENARIOS, activeScenario, confirmedByBuild, hasVisibilityRules, partitionRelevant } from './scenario';

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

describe('confirmedByBuild', () => {
  const context = { main_skill: 'FrostBombPlayer', skill_types: [], predicates: { if_cond: { Chilled: true } } };
  const parent = option('conditionChilled', { if_cond: ['Chilled'] });
  const dependent = option('conditionChilledEffect', { if_option: ['conditionChilled'] });
  const both = option('both', { if_cond: ['Chilled'], if_option: ['conditionChilled'] });

  it('promotes what the calculation confirmed, and never an option that only depends on another option', () => {
    const split = partitionRelevant([parent, dependent, both], confirmedByBuild(context));
    expect(split.relevant.map(candidate => candidate.var)).toEqual(['conditionChilled']);
    expect(split.rest.map(candidate => candidate.var)).toEqual(['conditionChilledEffect', 'both']);
  });

  it('is the same before and after the user sets the option it depends on', () => {
    // The dependent is shown once its parent is set (configApplicability with the inputs), yet stays in its own group.
    expect(configApplicability(dependent, context, {})).toBe('not_applicable');
    expect(configApplicability(dependent, context, { conditionChilled: true })).toBe('applicable');
    expect(confirmedByBuild(context)(dependent)).toBe('not_applicable');
  });

  it('keeps an option the reader changed where it was, whatever the context says now', () => {
    // Setting `parent` made the calculation read it: it is confirmed now, but it was not in the leading group when changed.
    const stay = partitionRelevant([parent, dependent], confirmedByBuild(context, { conditionChilled: false, conditionChilledEffect: true }));
    expect(stay.relevant.map(candidate => candidate.var)).toEqual(['conditionChilledEffect']);
    expect(stay.rest.map(candidate => candidate.var)).toEqual(['conditionChilled']);
    // An option nobody changed is still read from the context.
    expect(partitionRelevant([parent], confirmedByBuild(context, { other: false })).relevant).toHaveLength(1);
  });
});
