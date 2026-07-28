import { describe, expect, test } from 'vitest';
import { gemTagLabels, gemTagMatches } from './gemTags';

const FIREBALL_TAGS = ['Area', 'AreaSpell', 'Damage', 'Fire', 'Projectile', 'Spell'];

describe('gemTagLabels', () => {
  test('shows whitelisted tags in priority order, localized', () => {
    expect(gemTagLabels(FIREBALL_TAGS, 'en-US')).toEqual(['Spell', 'Projectile', 'Area', 'Fire']);
    expect(gemTagLabels(FIREBALL_TAGS, 'zh-CN')).toEqual(['法术', '投射物', '范围', '火焰']);
  });

  test('drops internal engine jargon entirely', () => {
    expect(gemTagLabels(['CrossbowAmmoSkill', 'AttackInPlace'], 'en-US')).toEqual([]);
  });
});

describe('gemTagMatches', () => {
  test('matches raw engine word and every locale label', () => {
    expect(gemTagMatches(FIREBALL_TAGS, 'spell')).toBe(true);
    expect(gemTagMatches(FIREBALL_TAGS, '法术')).toBe(true);
    expect(gemTagMatches(FIREBALL_TAGS, '法術')).toBe(true);
    expect(gemTagMatches(FIREBALL_TAGS, '闪电')).toBe(false);
  });
});
