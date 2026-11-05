import { expect, it } from 'vitest';
import { contributionLabel, removalChange } from './sourceContributions';

it('shows removal as a loss for helpful sources and a gain for harmful sources', () => {
  expect(removalChange(2290002)).toBe(-2290002);
  expect(removalChange(-200)).toBe(200);
});

it.each([
  ['en-US', 'Equipment · Main Hand', 'Skill group 2'],
  ['zh-CN', '装备 · 主手', '技能组 2'],
  ['zh-TW', '裝備 · 主手', '技能組 2'],
] as const)('labels equipment and skill groups once in %s', (lang, equipment, group) => {
  expect(contributionLabel(lang, 'item', 'weapon1')).toBe(equipment);
  expect(contributionLabel(lang, 'socket_group', '1')).toBe(group);
});
