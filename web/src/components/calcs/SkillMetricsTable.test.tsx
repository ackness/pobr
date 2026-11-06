import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { expect, test, vi } from 'vitest';
import type { GemCatalogEntry, SkillMetricsEntry, SkillMetricsResponse } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { SkillMetricsTable, isNonDamagingSkill, isSummonerSkill } from './SkillMetricsTable';

const harness = vi.hoisted(() => ({ catalog: new Map<string, GemCatalogEntry>(),
  clicks: [] as Array<NonNullable<ComponentProps<'button'>['onClick']>> }));
vi.mock('../../hooks/useSkillName', () => ({ useSkillCatalog: () => harness.catalog, useSkillName: () => (id: string) => id }));
vi.mock('../shared/controls', () => ({ AppButton: ({ onClick, ...props }: ComponentProps<'button'>) => {
  harness.clicks.push(onClick!);
  return <button {...props} />;
} }));

const metrics = { combined_dps: 0, hit_dps: 0, dot_dps: 0, average_hit: 0, action_rate: 1, mana_cost: 20, life_cost: 0, cooldown: 0 };
const row = (skill_id: string, index = 0): SkillMetricsEntry => ({ group_index: index, active_skill_index: 1,
  skill_id, status: 'calculated', metrics, error: null, unsupported_modifiers: ['Shared limitation'] });
const gem = (skill_id: string, tags: string[]): GemCatalogEntry => ({ skill_id, tags, name: skill_id,
  active_effect_ids: [skill_id], is_support: false, is_lineage: false, colour: null, name_zh_cn: null, name_zh_tw: null });

test('zero damage is not enough to label a skill non-damaging, and secondary effects do not borrow the host tags', () => {
  const catalog = new Map([['Curse', gem('Curse', ['Spell', 'AppliesCurse'])],
    ['Fireball', gem('Fireball', ['Spell', 'Damage', 'Projectile'])],
    ['Minion', gem('Minion', ['Persistent', 'Minion'])]]);
  expect(isNonDamagingSkill(row('Curse'), catalog)).toBe(true);
  expect(isNonDamagingSkill(row('Fireball'), catalog)).toBe(false);
  expect(isNonDamagingSkill(row('Minion'), catalog)).toBe(false);
  expect(isNonDamagingSkill(row('Secondary'), catalog)).toBe(false);
  expect(isNonDamagingSkill({ ...row('Curse'), metrics: { ...metrics, combined_dps: 10 } }, catalog)).toBe(false);
  expect(isNonDamagingSkill({ ...row('Curse'), status: 'unavailable', metrics: null }, catalog)).toBe(false);
});

test('overview deduplicates limitations, keeps errors in collapsed details and preserves numeric costs', () => {
  harness.catalog = new Map([['Curse', gem('Curse', ['Spell', 'AppliesCurse'])], ['Fireball', gem('Fireball', ['Damage', 'Spell'])]]);
  const report: SkillMetricsResponse = { item_errors: [], skills: [row('Curse'), row('Fireball', 1),
    { ...row('Unknown', 2), status: 'unavailable', metrics: null, error: 'Cannot resolve effect EnemyExplode' }] };
  const inspectSkill = vi.fn();
  const session = { socketGroups: report.skills.map(() => ({ enabled: true, gems: [] })),
    calcParams: {}, stateVersion: 7, inspectSkill } as unknown as BuildSession;
  harness.clicks = [];
  const html = renderToStaticMarkup(<SkillMetricsTable session={session} lang="en-US" report={report} displayed={report} />);
  expect(html.match(/Shared limitation/g)).toHaveLength(1);
  expect(html).toContain('<details><summary>Details</summary><p>Cannot resolve effect EnemyExplode</p></details>');
  expect(html).toContain('Not supported yet');
  expect(html).not.toContain('<details open');
  const rows = html.match(/<tr class="skill-metrics-row[^>]*>[\s\S]*?<\/tr>/g)!;
  expect(rows[0]).toContain('Non-damaging skill');
  expect(rows[0]).toContain('<td class="mod-value">—</td><td class="mod-value">—</td>');
  expect(rows[0]).toContain('<td class="mod-value">20</td>');
  expect(rows[1]).not.toContain('Non-damaging skill');
  expect(rows[1]).toContain('<td class="mod-value">0</td>');
  harness.clicks[1]({ stopPropagation: vi.fn() } as never);
  expect(inspectSkill).toHaveBeenCalledWith(1, 1, 7);
  harness.clicks = [];
  renderToStaticMarkup(<SkillMetricsTable session={session} lang="en-US" report={null} displayed={report} />);
  harness.clicks[1]({ stopPropagation: vi.fn() } as never);
  expect(inspectSkill).toHaveBeenCalledTimes(1);
});

test.each([
  ['en-US', 'Spark (Group 1, effect 1)', 'Spark (Group 2, effect 1)', 'Spark (Group 2, effect 2)'],
  ['zh-CN', 'Spark（技能组 1，效果 1）', 'Spark（技能组 2，效果 1）', 'Spark（技能组 2，效果 2）'],
  ['zh-TW', 'Spark（技能組 1，效果 1）', 'Spark（技能組 2，效果 1）', 'Spark（技能組 2，效果 2）'],
] as const)('duplicate skills have distinct localized button names and inspect the exact instance in %s', (lang, ...labels) => {
  harness.catalog = new Map([['Spark', gem('Spark', ['Spell', 'Damage'])], ['Fireball', gem('Fireball', ['Spell', 'Damage'])]]);
  const report: SkillMetricsResponse = { item_errors: [], skills: [
    row('Spark', 0), row('Spark', 1), { ...row('Spark', 1), active_skill_index: 2 }, row('Fireball', 2),
  ] };
  const inspectSkill = vi.fn();
  const session = { socketGroups: [
    { enabled: true, gems: [] }, { enabled: true, gems: [], main_active_skill: 2 }, { enabled: true, gems: [] },
  ], calcParams: { main_socket_group: 1 }, stateVersion: 7, inspectSkill } as unknown as BuildSession;
  harness.clicks = [];
  const html = renderToStaticMarkup(<SkillMetricsTable session={session} lang={lang} report={report} displayed={report} />);
  const buttons = [...html.matchAll(/<button[^>]*>([^<]*)<\/button>/g)];
  expect(buttons.map(button => button[1])).toEqual([...labels, 'Fireball']);
  expect(new Set(buttons.map(button => button[1])).size).toBe(4);
  expect(buttons[2][0]).toContain('aria-pressed="true"');
  for (let index = 0; index < 3; index++) harness.clicks[index]({ stopPropagation: vi.fn() } as never);
  expect(inspectSkill.mock.calls).toEqual([[0, 1, 7], [1, 1, 7], [1, 2, 7]]);
});

test('a summoning skill shows its minion\'s damage only when it is the calculated skill', () => {
  harness.catalog = new Map([['WolfPack', gem('WolfPack', ['Minion', 'CreatesMinion', 'Persistent'])], ['Zombie', gem('Zombie', ['Minion', 'CreatesMinion'])]]);
  expect(isSummonerSkill(row('WolfPack'), harness.catalog)).toBe(true);
  expect(isSummonerSkill({ ...row('WolfPack'), metrics: { ...metrics, combined_dps: 10 } }, harness.catalog)).toBe(false);
  const report: SkillMetricsResponse = { item_errors: [], skills: [row('WolfPack'), row('Zombie', 1)] };
  const session = { socketGroups: report.skills.map(() => ({ enabled: true, gems: [] })), calcParams: { main_socket_group: 0 }, stateVersion: 1,
    calc: { main_skill: { group_index: 0, active_skill_index: 1 }, main_minion_index: 4, minions: [{ index: 4, dps: 5307.37 }] },
    inspectSkill: vi.fn() } as unknown as BuildSession;
  const html = renderToStaticMarkup(<SkillMetricsTable session={session} lang="en-US" report={report} displayed={report} />);
  const [wolf, zombie] = html.split('<tr class="skill-metrics-row').slice(1);
  expect(wolf).toContain('minion hit DPS, one minion');
  expect(wolf).toContain('<td class="mod-value">5,307</td>');
  expect(zombie).toContain('Make it the calculated skill to see their damage.');
  // Its own zero damage and average hit are not figures; costs and speed still are.
  expect(zombie).toContain('<td class="mod-value">—</td><td class="mod-value">—</td><td class="mod-value">1</td>');
});

test('a damaging summoner still shows its minion, and the highest-DPS mark compares the shown values', () => {
  harness.catalog = new Map([['Unearth', gem('Unearth', ['Spell', 'Damage', 'CreatesMinion'])], ['Fireball', gem('Fireball', ['Spell', 'Damage'])]]);
  const report: SkillMetricsResponse = { item_errors: [], skills: [
    { ...row('Unearth'), metrics: { ...metrics, combined_dps: 400, average_hit: 120 } },
    { ...row('Fireball', 1), metrics: { ...metrics, combined_dps: 600 } },
  ] };
  const session = { socketGroups: report.skills.map(() => ({ enabled: true, gems: [] })), calcParams: { main_socket_group: 0 }, stateVersion: 1,
    calc: { main_skill: { group_index: 0, active_skill_index: 1 }, main_minion_index: 0, minions: [{ index: 0, dps: 5307.37 }] },
    inspectSkill: vi.fn() } as unknown as BuildSession;
  const html = renderToStaticMarkup(<SkillMetricsTable session={session} lang="en-US" report={report} displayed={report} />);
  const [unearth, fireball] = html.split('<tr class="skill-metrics-row').slice(1);
  expect(unearth).toContain('minion hit DPS, one minion');
  expect(unearth).toContain('<td class="mod-value">5,307</td>');
  expect(unearth).toContain('Highest DPS');
  expect(fireball).not.toContain('Highest DPS');
});
