import { expect, it } from 'vitest';
import type { CalculateBuildResponse, GemCatalogEntry, SkillMetricsEntry, SocketGroupInput } from '../api/types';
import { calculatedSkillOptions } from './calculatedSkillOptions';
const groups: SocketGroupInput[] = ['Fireball', 'Buff', 'Fireball', 'Unknown'].map(skill_id => ({ enabled: true, gems: [{ skill_id, level: 20, quality: 0 }] }));
const catalog = new Map(['Fireball', 'Buff'].map(id => [id, { skill_id: id, name: id, is_support: false, active_effect_ids: [id], tags: id === 'Buff' ? ['Buff'] : ['Damage', 'Spell'] } as GemCatalogEntry]));
const metrics = [1000, 0, 2500, null].map((dps, group_index) => ({ group_index, skill_id: groups[group_index].gems[0].skill_id, active_skill_index: 1, status: dps === null ? 'unavailable' : 'calculated', metrics: dps === null ? null : { combined_dps: dps, average_hit: 0 } })) as SkillMetricsEntry[];
it('keeps skill names, group numbers, and correctly formatted DPS, with non-damaging skills last in their own group', () => {
  const rows = calculatedSkillOptions(groups, catalog, metrics, null, 'en-US');
  expect(rows.map(row => row.value)).toEqual(['0', '2', '3', '1']);
  expect(rows[0].label).toBe('Fireball · 1,000 Combined DPS · Group 1');
  expect(rows[1].label).toBe('Fireball · 2,500 Combined DPS · Group 3');
  expect(rows[2].label).toContain('Not supported yet: Unknown');
  expect(rows[3].group).toBe('Other (non-damaging skills)');
});
it('keeps an unresolved imported skill name', () => {
  const rows = calculatedSkillOptions([{ enabled: true, gems: [{ skill_id: '', name_spec: 'Imported skill', level: 1, quality: 0 }] }], new Map(), [], null, 'en-US');
  expect(rows[0].label).toContain('Not supported yet: Imported skill');
});
it('uses the main minion DPS and keeps pending summoners out of the non-damaging group', () => {
  const catalog = new Map([['Buff', { name: 'Buff', active_effect_ids: ['Buff'], tags: ['CreatesMinion'] } as GemCatalogEntry]]);
  const calc = { main_skill: { group_index: 1, active_skill_index: 1, combined_dps: 0 }, main_minion_index: 0, minions: [{ index: 0, dps: 1234 }] } as CalculateBuildResponse;
  const row = calculatedSkillOptions(groups, catalog, metrics, calc, 'en-US').find(row => row.value === '1')!;
  expect(row.label).toContain('1,234 Minion hit DPS');
  expect(row.group).toBe('Damage and summoning skills');
  const unselected = calculatedSkillOptions(groups, catalog, metrics, null, 'en-US').find(row => row.value === '1')!;
  expect(unselected.group).toBe('Damage and summoning skills');
  expect(unselected.label).toBe('Buff · Summons minions. Make it the calculated skill to see their damage. · Group 2');
  expect(unselected.label).not.toContain('0 Combined DPS');
  const unresolved = { ...calc, minions: [], main_minion_index: null };
  expect(calculatedSkillOptions(groups, catalog, metrics, unresolved, 'en-US').find(row => row.value === '1')!.label)
    .toContain('Summons minions. Make it the calculated skill to see their damage.');
});

it('keeps a calculated zero-DPS damage skill in the damage group and shows its actual zero', () => {
  const zeroDamage = [{ ...metrics[0], metrics: { ...metrics[0].metrics!, combined_dps: 0, average_hit: 0 } }];
  const row = calculatedSkillOptions(groups, catalog, zeroDamage, null, 'en-US')[0];
  expect(row.value).toBe('0');
  expect(row.group).toBe('Damage and summoning skills');
  expect(row.label).toBe('Fireball · 0 Combined DPS · Group 1');
  expect(row.label).not.toContain('pending');
});
