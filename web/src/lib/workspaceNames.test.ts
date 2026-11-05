import { expect, test } from 'vitest';
import { buildDisplayName, importedBuildName, stageDisplayName } from './workspaceNames';

test.each([
  ['en-US', 'Build', 'Stage'], ['zh-CN', 'BD', '阶段'], ['zh-TW', 'BD', '階段'],
] as const)('unnamed builds and stages share numbered display names in %s', (lang, build, stage) => {
  expect(buildDisplayName({ name: '' }, 0, lang)).toBe(`${build} 1`);
  expect(buildDisplayName({ name: '' }, 2, lang)).toBe(`${build} 3`);
  expect(stageDisplayName({ name: '' }, 0, lang)).toBe(`${stage} 1`);
  expect(stageDisplayName({ name: '' }, 1, lang)).toBe(`${stage} 2`);
  expect(buildDisplayName({ name: 'My custom BD' }, 0, lang)).toBe('My custom BD');
  expect(stageDisplayName({ name: 'Current' }, 0, lang)).toBe('Current');
});

test('import naming uses locale-pack ascendancy names and preserves explicit and planner names', () => {
  const character = { class_name: 'Witch', ascendancy_name: 'Blood Mage' };
  const names = { classes: { Witch: '女巫' }, ascendancies: { 'Blood Mage': '命源法师' },
    locales: { 'zh-TW': { classes: { Witch: '女巫' }, ascendancies: { 'Blood Mage': '血法師' } } } };
  expect(importedBuildName(character, names, 'zh-CN')).toBe('命源法师');
  expect(importedBuildName(character, names, 'zh-TW')).toBe('血法師');
  expect(importedBuildName(character, names, 'en-US')).toBe('Blood Mage');
  expect(importedBuildName({ ...character, ascendancy_name: '' }, names, 'zh-CN')).toBe('女巫');
  expect(importedBuildName(character, names, 'zh-CN', ' Custom name ', 'Planner name')).toBe('Custom name');
  expect(importedBuildName(character, names, 'zh-CN', ' ', ' Planner name ')).toBe('Planner name');
});
