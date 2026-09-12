import { describe, expect, test } from 'vitest';
import type { CalculateBuildResponse, GemCatalogEntry } from '../api/types';
import { buildProfile, compareGems, keyPassives, matchBuild, ninjaSearchUrl, resistanceGaps, type BuildProfile } from './buildGuidance';

const catalog = new Map([
  ['FireballPlayer', { skill_id: 'FireballPlayer', name: 'Fireball', name_zh_cn: '火球', is_support: false } as GemCatalogEntry],
  ['IceShotPlayer', { skill_id: 'IceShotPlayer', name: 'Ice Shot', is_support: false } as GemCatalogEntry],
  ['SupportA', { skill_id: 'SupportA', name: 'Support A', is_support: true } as GemCatalogEntry],
]);
const profile: BuildProfile = { className: 'Sorceress', ascendancy: 'Stormweaver', mainSkill: 'FireballPlayer', supports: ['SupportA'], uniques: ['Example Unique'] };

describe('build reference matching', () => {
  test('empty and unknown data do not earn similarity points', () => {
    const empty = buildProfile({}, catalog);
    expect(matchBuild(empty, empty).score).toBe(0);
    expect(buildProfile({ main_socket_group: 0, socket_groups: [{ enabled: true, gems: [{ skill_id: 'Unknown', level: 1, quality: 0 }] }] }, catalog).mainSkill).toBeNull();
    expect(buildProfile({ main_socket_group: 0, socket_groups: [{ enabled: true, gems: [{ skill_id: 'Unknown', level: 1, quality: 0 }] }] }, catalog, 'Unknown').mainSkill).toBeNull();
  });
  test('main skill outweighs unrelated signals and supports only count on that skill', () => {
    const sameSkill = matchBuild(profile, { ...profile, className: 'Witch', ascendancy: 'Blood Mage', supports: [], uniques: [] });
    const unrelated = matchBuild(profile, { ...profile, mainSkill: 'IceShotPlayer' });
    expect(sameSkill.score).toBeGreaterThan(unrelated.score);
    expect(unrelated.supportOverlap).toBe(0);
    expect(matchBuild(profile, profile).score).toBe(100);
    expect(matchBuild({ ...profile, supports: [], uniques: [] }, { ...profile, supports: [], uniques: [] }).score).toBe(75);
  });
  test('set overlap does not inflate duplicates or stack class and ascendancy', () => {
    const match = matchBuild(profile, { ...profile, supports: ['SupportA', 'SupportA', 'SupportB'], uniques: [] });
    expect(match.supportOverlap).toBe(0.5);
    expect(match.score).toBe(83);
  });
  test('uses edited groups and rejects a stale calculated skill or disabled group', () => {
    const request = { main_socket_group: 0, socket_groups: [{ enabled: true, gems: [{ skill_id: 'IceShotPlayer', level: 12, quality: 10 }, { skill_id: 'SupportA', level: 1, quality: 0 }] }] };
    expect(buildProfile(request, catalog, 'FireballPlayer').mainSkill).toBe('IceShotPlayer');
    expect(buildProfile({ ...request, socket_groups: [{ ...request.socket_groups[0], enabled: false }] }, catalog).supports).toEqual([]);
    expect(buildProfile({ ...request, main_socket_group: 4 }, catalog).mainSkill).toBeNull();
  });
});

test('ninja links use canonical names and bound the league path', () => {
  const url = new URL(ninjaSearchUrl('forbiddenrites', { ...profile, ascendancy: 'Gemling Legionnaire' }, catalog)!);
  expect(url.hostname).toBe('poe.ninja');
  expect(url.searchParams.get('skills')).toBe('Fireball');
  expect(url.searchParams.get('class')).toBe('Gemling Legionnaire');
  expect(new URL(ninjaSearchUrl('standard', profile, catalog, false)!).searchParams.has('class')).toBe(false);
  for (const league of ['../../evil', 'foo?class=X', 'https://example.test', '', 'a'.repeat(81)]) expect(ninjaSearchUrl(league, profile, catalog)).toBeNull();
});

test('gem comparison preserves missing gems, levels and quality on both sides', () => {
  const a = [{ skill_id: 'FireballPlayer', level: 12, quality: 0 }, { skill_id: 'SupportA', level: 1, quality: 0 }];
  const b = [{ skill_id: 'FireballPlayer', level: 20, quality: 20 }, { skill_id: 'SupportB', level: 2, quality: 0 }];
  const before = JSON.stringify([a, b]);
  const rows = compareGems(a, b);
  expect(rows[0].current?.level).toBe(12);
  expect(rows[0].reference?.quality).toBe(20);
  expect(rows.find(row => row.id === 'SupportB')?.current).toBeUndefined();
  expect(rows.find(row => row.id === 'SupportA')?.reference).toBeUndefined();
  expect(JSON.stringify([a, b])).toBe(before);
});

test('resistance gaps use the configured target, excluding unavailable results', () => {
  const calc = { stats: [{ id: 'FireResist', value: 65 }, { id: 'ColdResist', value: 80 }, { id: 'LightningResist', value: null }] } as CalculateBuildResponse;
  expect(resistanceGaps(calc, 75)).toEqual([{ stat: 'FireResist', value: 65, target: 75, gap: 10 }]);
  expect(resistanceGaps(calc, 60)).toEqual([]);
  expect(resistanceGaps(null, 75)).toEqual([]);
  expect(resistanceGaps(calc, Number.NaN)).toEqual([]);
});

test('passive details exclude travel nodes and unresolved ids', () => {
  expect(keyPassives([1, 2, 3, 99], [
    { skill: 1, id: 'small', kind: 'normal' }, { skill: 2, id: 'key', kind: 'keystone' },
    { skill: 3, id: 'notable', kind: 'notable' }, { skill: 4, id: 'other', kind: 'keystone' },
  ]).map(node => node.skill)).toEqual([2, 3]);
});
