import { expect, test } from 'vitest';
import type { GemCatalogEntry, SkillMetricsEntry, SocketGroupInput } from '../api/types';
import { activeSkillEffects, highestDpsSkill, preserveSkillSelection, removeSkillGem } from './skillSelection';

const gemDefaults = {
  name: 'Fixture', name_zh_tw: null, name_zh_cn: null, colour: null,
  is_support: false, is_lineage: false, tags: [],
};
const catalog = new Map<string, GemCatalogEntry>([
  ['Curse', { ...gemDefaults, skill_id: 'Curse', active_effect_ids: ['Curse'], additional_skill_ids: ['DisplayOnlyStatSet'] }],
  ['HexSupport', { ...gemDefaults, skill_id: 'HexSupport', active_effect_ids: ['DoomBlast'], is_support: true }],
  ['PureSupport', { ...gemDefaults, skill_id: 'PureSupport', active_effect_ids: [], is_support: true }],
  ['Wind', { ...gemDefaults, skill_id: 'Wind', active_effect_ids: ['Wind', 'TriggeredWind'] }],
]);
const group: SocketGroupInput = {
  enabled: true, weapon_set: 2, main_active_skill: 4,
  gems: ['Curse', 'PureSupport', 'HexSupport', '', 'Wind', 'Unknown'].map(skill_id => ({ skill_id, level: 1, quality: 0 })),
};

test('ordinals include granted effects and unknown IDs, not supports, empty slots or display aliases', () => {
  expect(activeSkillEffects(group, catalog)).toEqual([
    { skillId: 'Curse', gemIndex: 0 }, { skillId: 'DoomBlast', gemIndex: 2 },
    { skillId: 'Wind', gemIndex: 4 }, { skillId: 'TriggeredWind', gemIndex: 4 },
    { skillId: 'Unknown', gemIndex: 5 },
  ]);
});

test('removing a gem remaps every effect it supplied without changing the inspected effect or weapon binding', () => {
  const next = removeSkillGem(group, 2, catalog);
  expect(next.main_active_skill).toBe(3);
  expect(activeSkillEffects(next, catalog)[next.main_active_skill! - 1].skillId).toBe('TriggeredWind');
  expect(next.weapon_set).toBe(2);
  expect(removeSkillGem(group, 1, catalog).main_active_skill).toBe(4);
  expect(removeSkillGem(group, 3, catalog).main_active_skill).toBe(4);
  expect(group.gems).toHaveLength(6);
});

test('removing the selected multi-effect gem selects a surviving preceding effect and preserves invalid ordinals', () => {
  const next = removeSkillGem(group, 4, catalog);
  expect(next.main_active_skill).toBe(2);
  expect(activeSkillEffects(next, catalog)[1].skillId).toBe('DoomBlast');
  expect(removeSkillGem({ ...group, main_active_skill: 99 }, 1, catalog).main_active_skill).toBe(99);
  const empty = removeSkillGem({ enabled: true, main_active_skill: 2, gems: [group.gems[4]] }, 0, catalog);
  expect(empty.gems).toEqual([]);
  expect(empty.main_active_skill).toBeUndefined();
});

test('level and quality comparisons retain the second copy of a skill rather than borrowing the first', () => {
  const original: SocketGroupInput = { enabled: true, main_active_skill: 2,
    gems: [group.gems[0], { ...group.gems[0], level: 10 }] };
  const replacement = { ...original, gems: [original.gems[0], { ...original.gems[1], quality: 20 }] };
  expect(preserveSkillSelection(original, replacement, catalog)?.main_active_skill).toBe(2);
  expect(original.gems[1].quality).toBe(0);
});

function metricsRow(group_index: number, active_skill_index: number, combined_dps: number | null): SkillMetricsEntry {
  return {
    group_index, active_skill_index, skill_id: `Skill${group_index}_${active_skill_index}`,
    status: combined_dps === null ? 'unavailable' : 'calculated',
    metrics: combined_dps === null ? null : {
      hit_dps: combined_dps, dot_dps: 0, combined_dps, average_hit: 0, action_rate: 1, mana_cost: 0, life_cost: 0, cooldown: 0,
    },
    unsupported_modifiers: [], error: null,
  };
}
const groupStates = [{ enabled: true }, { enabled: true }, { enabled: false }];

test('default skill is the highest combined DPS among enabled, calculated effects', () => {
  const skills = [metricsRow(0, 1, 100), metricsRow(0, 2, 900), metricsRow(1, 1, 400), metricsRow(2, 1, 5000), metricsRow(1, 2, null)];
  expect(highestDpsSkill(skills, groupStates)).toBe(skills[1]);
});

test('default skill keeps the earliest row on ties and utility-only builds', () => {
  const skills = [metricsRow(1, 1, 0), metricsRow(0, 1, 0)];
  expect(highestDpsSkill(skills, groupStates)).toBe(skills[0]);
  expect(highestDpsSkill([metricsRow(0, 1, 50), metricsRow(1, 1, 50)], groupStates)?.group_index).toBe(0);
});

test('no calculable skill leaves the selection empty', () => {
  expect(highestDpsSkill([metricsRow(0, 1, null), metricsRow(2, 1, 10)], groupStates)).toBeNull();
});
