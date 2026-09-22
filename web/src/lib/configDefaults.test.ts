import { readFileSync } from 'node:fs';
import { expect, test } from 'vitest';
import type { ConfigOption } from '../api/types';
import { configDefaultValue } from './configDefaults';
import { CONFIG_LABEL_ZH } from './configLabels';

const version = readFileSync(new URL('../../../data/CURRENT', import.meta.url), 'utf8').trim();
const options: ConfigOption[] = JSON.parse(readFileSync(new URL(`../../../data/${version}/overlay/config_options.json`, import.meta.url), 'utf8')).options;

test('real catalog fixed quest rewards and combat defaults match their typed states', () => {
  const fixed = options.filter(option => option.section === 'Quest Rewards' && option.input_type === 'check');
  expect(fixed.length).toBeGreaterThan(0);
  for (const option of fixed) expect(configDefaultValue(option), option.var).toBe(true);
  expect(configDefaultValue(options.find(option => option.var === 'companionInPresence')!)).toBe(true);
  expect(configDefaultValue(options.find(option => option.var === 'conditionEnemyShocked')!)).toBe(false);
  for (const option of options.filter(option => option.section === 'Quest Rewards' && option.input_type === 'list')) {
    expect(configDefaultValue(option), option.var).toBe('None');
  }
});

test('numeric defaults preserve zero and distinguish state from placeholder precedence', () => {
  const count: ConfigOption = { var: 'count', input_type: 'count', default: { state_number: 0, placeholder_number: 20 } };
  expect(configDefaultValue(count)).toBe(0);
  expect(configDefaultValue({ ...count, default: { placeholder_number: 20 } })).toBe(20);
  expect(configDefaultValue({ ...count, default: 10 })).toBe(10);
  expect(configDefaultValue({ var: 'choice', input_type: 'list', default: { index: 2 },
    list_options: [{ value: 'a', label: 'A' }, { value: 'b', label: 'B' }] })).toBe('b');
});

test('every current configuration label has a Chinese display translation', () => {
  expect(options.filter(option => !CONFIG_LABEL_ZH[option.var]).map(option => option.var)).toEqual([]);
});
