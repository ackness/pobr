import { expect, test } from 'vitest';
import { configListLabel, configValueLabel } from './configDisplay';
import type { ConfigOption } from '../api/types';

test.each(['en-US', 'zh-CN', 'zh-TW'] as const)('list display reuses configuration labels in %s', lang => {
  const option: ConfigOption = { var: 'cadiroAmmo', input_type: 'list', list_options: [{ value: 'arrow-id', label: '^1Diamond Arrow^7' }] };
  expect(configValueLabel(lang, option, 'arrow-id')).toBe(configListLabel(lang, 'Diamond Arrow'));
  expect(configValueLabel(lang, undefined, undefined)).not.toContain('undefined');
  expect(configValueLabel(lang, { var: 'check', input_type: 'check' }, 0)).not.toBe('0');
  expect(configValueLabel(lang, undefined, 0)).toBe('0');
});

test('multi-line list choices use shared translations with no array or markup syntax', () => {
  const option: ConfigOption = { var: 'quest', input_type: 'list', list_options: [{ value: 'choice', label: '^1First reward^7\nSecond reward' }] };
  expect(configValueLabel('zh-CN', option, 'choice', { 'First reward': '奖励一', 'Second reward': '奖励二' })).toBe('奖励一 / 奖励二');
});

test.each([
  ['en-US', 'Saved choice is not in the current list (see advanced details)'],
  ['zh-CN', '已保存的选项不在当前列表中（见高级详情）'],
  ['zh-TW', '已儲存的選項不在目前清單中（見進階詳情）'],
] as const)('unmatched list values and unlabelled choices use a localized fallback in %s', (lang, fallback) => {
  const option: ConfigOption = { var: 'knownList', input_type: 'list', list_options: [{ value: 'currentEnum', label: 'Current choice' }, { value: 'blankLabel', label: '^7 ' }] };
  for (const value of ['retiredInternalEnum', false, 42, 'blankLabel'] as const) expect(configValueLabel(lang, option, value)).toBe(fallback);
  expect(configValueLabel(lang, { ...option, var: 'enemyIsBoss' }, 'retiredEnemyTier')).toBe(fallback);
  expect(configValueLabel(lang, option, 'currentEnum')).toBe('Current choice');
});
