import { renderToStaticMarkup } from 'react-dom/server';
import { expect, test, vi } from 'vitest';
import { ConfigComparison } from './ConfigComparison';
import { useLocalizedLines } from '../../hooks/useLocalizedLines';
import type { ConfigOption } from '../../api/types';

const catalog = vi.hoisted(() => ({ options: [
  { var: 'conditionEnemyShocked', input_type: 'check', label: 'Is the enemy Shocked?' },
  { var: 'enemyIsBoss', input_type: 'list', label: 'Enemy tier', list_options: [{ value: 'pinnacle', label: 'Pinnacle Boss' }] },
  { var: 'unlabelledCatalogVar', input_type: 'list', list_options: [{ value: 'currentEnum', label: 'Current choice' }] },
  { var: 'blankCatalogLabel', label: '^7 ', input_type: 'check' },
  { var: 'questAct 1ClearfellBeira', input_type: 'check', section: 'Quest Rewards', label: 'Act 1: Clearfell', tooltip: '^x88FFFFBeira^7\n+10% to Cold Resistance' },
] as ConfigOption[] }));
vi.mock('react', async original => ({ ...await original<typeof import('react')>(),
  useState: (initial: unknown) => [initial && typeof initial === 'object' && 'options' in initial ? catalog : initial, () => {}],
}));
vi.mock('../../hooks/useLocalizedLines', async original => ({
  ...await original<typeof import('../../hooks/useLocalizedLines')>(),
  useLocalizedLines: vi.fn((lines: string[], lang: string) => {
    const labels: Record<string, Record<string, string>> = {
      'zh-CN': { '+10% to Cold Resistance': '+10% 冰冷抗性', '+10 to maximum Life': '+10 最大生命',
        '+5 to Strength': '+5 力量', 'All Damage is taken from Mana before Life': '受到的所有伤害在扣除生命前优先从魔力扣除，魔力回复速度总降 50%',
        '50% less Mana Recovery Rate': '魔力回复速度总降 50%' },
      'zh-TW': { '+10% to Cold Resistance': '+10% 冰冷抗性', '+10 to maximum Life': '+10 最大生命',
        '+5 to Strength': '+5 力量', 'All Damage is taken from Mana before Life': '受到的所有傷害在扣除生命前優先從魔力扣除，魔力回復速度總降 50%',
        '50% less Mana Recovery Rate': '魔力回復速度總降 50%' },
    };
    return lines.map((line, index) => lang !== 'en-US' && line === '50% less Mana Recovery Rate'
      && lines[index - 1] === 'All Damage is taken from Mana before Life' ? '' : labels[lang]?.[line] ?? line);
  }),
}));

test('configuration displays labels, yes/no, localized enemy tiers and quests with rewards; raw keys stay collapsed', () => {
  const html = renderToStaticMarkup(<ConfigComparison lang="zh-CN" applyButton={() => null} rows={[
    { key: 'conditionEnemyShocked', before: true, after: false, action: { kind: 'config', key: 'conditionEnemyShocked' } },
    { key: 'enemyIsBoss', before: 'pinnacle', after: 'none', action: { kind: 'config', key: 'enemyIsBoss' } },
    { key: 'questAct 1ClearfellBeira', before: false, after: true, action: { kind: 'config', key: 'questAct 1ClearfellBeira' } },
    { key: 'unrecognizedSavedKey', before: 'rawOld', after: 'rawNew', action: { kind: 'config', key: 'unrecognizedSavedKey' } },
    { key: 'Custom modifiers', before: [], after: [{ title: 'Test', enabled: false, text: '+10 to maximum Life' }], action: { kind: 'modifiers' } },
  ]} />);
  expect(html).toContain('第一章：净空林地');
  expect(html).toContain('+10% 冰冷抗性');
  expect(html).toContain('巅峰 Boss');
  expect(html).toContain('刷图');
  expect(html).toContain('>是</div>');
  expect(html).toContain('>否</div>');
  expect(html).toContain('>未设置</div>');
  expect(html).toContain('Test · 停用');
  expect(html).toContain('+10 最大生命');
  expect(html).not.toContain('<details open');
  const visible = html.replace(/<details>[^]*?<\/details>/g, '');
  expect(visible).not.toContain('rawOld');
  expect(visible).not.toContain('rawNew');
  expect(visible).not.toMatch(/>true<|>false<|>pinnacle<|>\[\]</);
  expect(html).toContain('rawOld');
  expect(html).toContain('rawNew');
});

test.each([
  ['en-US', 'Other saved setting', 'Saved choice is not in the current list (see advanced details)'],
  ['zh-CN', '其他已保存配置', '已保存的选项不在当前列表中（见高级详情）'],
  ['zh-TW', '其他已儲存設定', '已儲存的選項不在目前清單中（見進階詳情）'],
] as const)('missing catalog labels and retired enum values stay in collapsed details in %s', (lang, setting, choice) => {
  const rows = [
    { key: 'unlabelledCatalogVar', before: 'oldInternalEnum', after: 'currentEnum', action: { kind: 'config' as const, key: 'unlabelledCatalogVar' } },
    { key: 'blankCatalogLabel', before: false, after: true, action: { kind: 'config' as const, key: 'blankCatalogLabel' } },
  ];
  const html = renderToStaticMarkup(<ConfigComparison lang={lang} rows={rows} applyButton={() => null} />);
  const visible = html.replace(/<details>[^]*?<\/details>/g, '').replace(/<[^>]*>/g, '');
  expect(html).toContain(`<h4>${setting}</h4>`);
  expect(html.match(new RegExp(`<h4>${setting}</h4>`, 'g'))).toHaveLength(2);
  expect(visible).toContain(choice);
  expect(visible).toContain('Current choice');
  for (const raw of ['oldInternalEnum', 'unlabelledCatalogVar', 'blankCatalogLabel']) {
    expect(visible).not.toContain(raw);
    expect(html).toContain(raw);
  }
  expect(html).not.toContain('<details open');
  expect(rows[0].before).toBe('oldInternalEnum');
});

test.each([
  ['zh-CN', '魔力回复速度总降 50%', '原始文本'],
  ['zh-TW', '魔力回復速度總降 50%', '原始文字'],
] as const)('custom modifier strings and blocks use cleaned localized lines in %s', (lang, recovery, originalLabel) => {
  vi.mocked(useLocalizedLines).mockClear();
  const before = ['{range:0.5}^7+10 to maximum [Life|Life]\r\n+5 to Strength'];
  const after = [
    { title: '{custom}^7Test block', enabled: true, text: 'All Damage is taken from Mana before Life\n50% less Mana Recovery Rate\n\n{tag}Unknown [internal|effect]' },
    { title: 'Separate block', enabled: false, text: '50% less Mana Recovery Rate' },
  ];
  const original = JSON.stringify({ before, after });
  const html = renderToStaticMarkup(<ConfigComparison lang={lang} applyButton={() => null} rows={[
    { key: 'Custom modifiers', before, after, action: { kind: 'modifiers' } },
  ]} />);
  const visible = html.replace(/<details>[^]*?<\/details>/g, '');
  expect(visible).toContain(`<p lang="${lang}">+10 最大生命</p>`);
  expect(visible).toContain(`<p lang="${lang}">+5 力量</p>`);
  expect(visible).not.toContain('+10 to maximum Life');
  expect(visible).not.toContain('All Damage is taken from Mana before Life');
  expect(visible).not.toContain('50% less Mana Recovery Rate');
  expect(visible).toContain('Test block');
  expect(visible).toContain(`<p lang="en">Unknown effect<span class="cmp-note"> · ${originalLabel}</span></p>`);
  expect(visible.match(new RegExp(recovery, 'g'))).toHaveLength(2);
  for (const raw of ['{range:0.5}', '^7', '[Life|Life]', '{tag}', '[internal|effect]', '{custom}']) {
    expect(visible).not.toContain(raw);
    expect(html).toContain(raw);
  }
  expect(useLocalizedLines).toHaveBeenCalledWith(['+10 to maximum Life', '+5 to Strength'], lang);
  expect(useLocalizedLines).toHaveBeenCalledWith(['All Damage is taken from Mana before Life', '50% less Mana Recovery Rate', '', 'Unknown effect'], lang);
  expect(useLocalizedLines).toHaveBeenCalledWith(['50% less Mana Recovery Rate'], lang);
  expect(html).not.toContain('<details open');
  expect(JSON.stringify({ before, after })).toBe(original);
});

test('English modifier display cleans annotations without adding fallback markers', () => {
  const html = renderToStaticMarkup(<ConfigComparison lang="en-US" applyButton={() => null} rows={[
    { key: 'Custom modifiers', before: [], after: ['{tag}^7+10 to maximum [Life|Life]'], action: { kind: 'modifiers' } },
  ]} />);
  const visible = html.replace(/<details>[^]*?<\/details>/g, '');
  expect(visible).toContain('<p lang="en">+10 to maximum Life</p>');
  expect(visible).not.toContain('Original text');
});

test.each([
  ['zh-CN', '受到的所有伤害在扣除生命前优先从魔力扣除，魔力回复速度总降 50%'],
  ['zh-TW', '受到的所有傷害在扣除生命前優先從魔力扣除，魔力回復速度總降 50%'],
] as const)('adjacent legacy array entries reach the translation hook together in %s', (lang, description) => {
  vi.mocked(useLocalizedLines).mockClear();
  const before = ['All Damage is taken from Mana before Life', '50% less Mana Recovery Rate'];
  const html = renderToStaticMarkup(<ConfigComparison lang={lang} applyButton={() => null} rows={[
    { key: 'Custom modifiers', before, after: [], action: { kind: 'modifiers' } },
  ]} />);
  expect(useLocalizedLines).toHaveBeenCalledWith(before, lang);
  for (const line of before) expect(useLocalizedLines).not.toHaveBeenCalledWith([line], lang);
  const visible = html.replace(/<details>[^]*?<\/details>/g, '');
  expect(visible).toContain(`<p lang="${lang}">${description}</p>`);
  expect(visible.match(/<p lang=/g)).toHaveLength(1);
  for (const line of before) {
    expect(visible).not.toContain(line);
    expect(html).toContain(line);
  }
  expect(before).toEqual(['All Damage is taken from Mana before Life', '50% less Mana Recovery Rate']);
});
