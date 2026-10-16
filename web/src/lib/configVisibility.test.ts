import { expect, test } from 'vitest';
import { readFileSync } from 'node:fs';
import type { ConfigContext, ConfigOption } from '../api/types';
import { configApplicability, configExistsInGame, configVisibilityInputs, showConfigOption } from './configVisibility';
import gameRelevance from './configGameRelevance.json';

const option: ConfigOption = { var: 'skillSetting', input_type: 'count', visibility: { if_skill: ['Fireball'] } };
const context: ConfigContext = { main_skill: 'spark', skill_types: ['Spell'], predicates: {
  if_skill: { Spark: true, Fireball: false }, if_skill_type: { Spell: true, Attack: false },
  if_cond: { LowLife: true, KilledRecently: false }, if_stat: { Life: true }, if_flag: { attack: false, spell: true },
} };

test('game-data exclusions apply before all-options and saved-input visibility', () => {
  const brand: ConfigOption = { var: 'ActiveBrands', input_type: 'count' };
  const saved = { ActiveBrands: 3 };
  expect(showConfigOption(brand, 'unknown', saved, true)).toBe(true);
  expect(configExistsInGame(brand, gameRelevance.data_version)).toBe(false);
  expect(saved).toEqual({ ActiveBrands: 3 });
  expect(configExistsInGame(brand, 'a-new-snapshot')).toBe(true);
  expect(configExistsInGame(brand, undefined)).toBe(true);
  for (const var_ of ['conditionShockEffect', 'detonateDeadCorpseLife', 'SecondsSinceInevitableCrit', 'conditionUnravelling']) {
    expect(configExistsInGame({ var: var_, input_type: 'count' }, gameRelevance.data_version)).toBe(true);
  }
});

test('irrelevant skill option hides unless all options are requested', () => {
  const applicability = configApplicability(option, context, {});
  expect(applicability).toBe('not_applicable');
  expect(showConfigOption(option, applicability, {}, false)).toBe(false);
  expect(showConfigOption(option, applicability, {}, true)).toBe(true);
});

test.each([false, 0, '', true, 2])('explicit %j is preserved even when irrelevant', value => {
  expect(showConfigOption(option, 'not_applicable', { skillSetting: value }, false)).toBe(true);
});

test('catalog defaults are not saved inputs', () => {
  const withDefault = { ...option, default: { state_number: 0 } };
  expect(showConfigOption(withDefault, 'not_applicable', {}, false)).toBe(false);
});

test('predicate alternatives use OR and families use AND', () => {
  expect(configApplicability({ ...option, visibility: { if_skill: ['Fireball', 'Spark'], if_skill_type: ['Spell'] } }, context, {})).toBe('applicable');
  expect(configApplicability({ ...option, visibility: { if_skill: ['Spark'], if_flag: ['attack'] } }, context, {})).toBe('not_applicable');
});

test('condition visibility uses reads, independent of the value currently set', () => {
  expect(configApplicability({ ...option, visibility: { if_cond: ['LowLife'], if_stat: ['Life'] } }, context, { conditionLowLife: false })).toBe('applicable');
});

test('unknown and unavailable context remain visible without claiming applicability', () => {
  expect(configApplicability(option, undefined, {})).toBe('unknown');
  expect(configApplicability({ ...option, visibility: { if_skill_data: ['unknown'], future_rule: ['x'] } }, context, {})).toBe('unknown');
  expect(showConfigOption(option, 'unknown', {}, false)).toBe(true);
  expect(configApplicability({ ...option, visibility: { if_skill: ['Fireball'], future_rule: ['x'] } }, context, {})).toBe('not_applicable');
});

test('dependent option preserves catalog truthiness for zero and false', () => {
  const dependent = { ...option, visibility: { if_option: ['other'] } };
  expect(configApplicability(dependent, context, {})).toBe('not_applicable');
  expect(configApplicability(dependent, context, { other: false })).toBe('not_applicable');
  expect(configApplicability(dependent, context, { other: 0 })).toBe('applicable');
});

test('dependency defaults do not become explicit overrides, and false overrides true', () => {
  const options: ConfigOption[] = [{ var: 'enabled', input_type: 'check', default: { state_bool: true } }];
  const dependent = { ...option, visibility: { if_option: ['enabled'] } };
  const saved = {};
  expect(configApplicability(dependent, context, configVisibilityInputs(options, saved))).toBe('applicable');
  expect(saved).toEqual({});
  expect(configApplicability(dependent, context, configVisibilityInputs(options, { enabled: false }))).toBe('not_applicable');
});

test('unconditional options work without a calculation', () => {
  expect(configApplicability({ ...option, visibility: undefined }, undefined, {})).toBe('applicable');
});

test.each([
  ['MetaCastOnBlockPlayer', ['energyIncomingHitRate']],
  ['MetaCastOnMinionDeathPlayer', ['energyMinionDeathsPerSecond', 'energyMinionPower']],
  ['MetaSpellslingerPlayer', ['energyInvocationUsesPerSecond']],
  ['MetaElementalInvocationPlayer', ['energyInvocationUsesPerSecond']],
  ['MetaCastOnElementalAilmentPlayer', []],
  ['MetaCastOnCritPlayer', []],
  ['SparkPlayer', []],
] as const)('Applicable view gates Energy controls for %s', (skill, expected) => {
  const native: { options: ConfigOption[] } = JSON.parse(readFileSync(new URL('../../../data/overlay-common/energy_config_options.json', import.meta.url), 'utf8'));
  const snapshot: { options: ConfigOption[] } = JSON.parse(readFileSync(new URL('../../../data/0.5.5.4.2/overlay/config_options.json', import.meta.url), 'utf8'));
  const energyOptions = native.options.filter(option => option.var.startsWith('energy'));
  const options = snapshot.options.filter(option => energyOptions.some(energy => energy.var === option.var));
  expect(options).toEqual([...energyOptions].sort((a, b) => a.var.localeCompare(b.var)));
  const ids = options.flatMap(option => option.visibility?.if_skill ?? []);
  // The backend contract tests verify these predicates from actual socket groups,
  // including a meta gem outside the inspected skill's group.
  const context: ConfigContext = { main_skill: 'spark', skill_types: ['Spell'], predicates: {
    if_skill: Object.fromEntries(ids.map(id => [id, id === skill])),
  } };
  const shown = options.filter(option => showConfigOption(option, configApplicability(option, context, {}), {}, false));
  expect(shown.map(option => option.var)).toEqual(expected);
  for (const option of shown) expect(configApplicability(option, context, {})).toBe('applicable');
});
