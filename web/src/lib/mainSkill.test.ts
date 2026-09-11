import { expect, test } from 'vitest';
import { defaultMainSkill } from './mainSkill';
import type { SocketGroupInput } from '../api/types';
const group = (skill_id: string, enabled = true): SocketGroupInput => ({ enabled, gems: [{ skill_id, level: 15, quality: 0 }] });
test('selects damage output instead of import order and ignores disabled groups', () => {
  expect(defaultMainSkill([group('ShieldWallPlayer'), group('FrostBombPlayer'), group('CometPlayer', false)], { full_dps: 99999, per_skill: [
    { group_index: 0, skill_id: 'ShieldWallPlayer', dps: 45 }, { group_index: 1, skill_id: 'FrostBombPlayer', dps: 815 }, { group_index: 2, skill_id: 'CometPlayer', dps: 90000 },
  ] })).toBe(1);
});
test('returns no selection for non-damage groups or invalid output', () => {
  expect(defaultMainSkill([group('AuraPlayer')], { full_dps: 0, per_skill: [] })).toBeUndefined();
  expect(defaultMainSkill([group('AuraPlayer')], { full_dps: 0, per_skill: [
    { group_index: 0, skill_id: 'AuraPlayer', dps: NaN }, { group_index: 10, skill_id: 'Missing', dps: 100 },
  ] })).toBeUndefined();
});
