import { expect, test, vi } from 'vitest';
import type { EvaluateOptions } from './optimize';
import { gemAcquisition, gemVariant, planGemUpgrades, usableGemLevel } from './tradeMarket';

vi.mock('../api/backend', () => ({ getBackend: async () => ({
  supportGroupsCompatible: async (groups: unknown[]) => groups.map(() => true),
  gemCatalog: async () => [
    { skill_id: 'Fireball', active_effect_ids: ['Fireball'] },
    ...['Old', 'Ordinary', 'Lineage', 'Attack', 'Unmodeled', 'Unknown'].map(skill_id => ({ skill_id, active_effect_ids: [] })),
  ],
}) }));

test('gem replacement preserves other supports and groups instead of appending duplicates', () => {
  const request = { socket_groups: [
    { enabled: true, gems: [{ skill_id: 'Fireball', level: 15, quality: 0 }, { skill_id: 'Old', level: 1, quality: 0 }] },
    { enabled: true, gems: [{ skill_id: 'Aura', level: 10, quality: 0 }] },
  ] };
  const variant = gemVariant(request, 0, 1, { skill_id: 'New', name: 'New', family: 'New', is_support: true, max_level: 1 }, 1, 20);
  expect(variant.socket_groups![0].gems.map(gem => gem.skill_id)).toEqual(['Fireball', 'New']);
  expect(variant.socket_groups![1]).toEqual(request.socket_groups[1]);
  expect(request.socket_groups[0].gems[1].skill_id).toBe('Old');
});

test('gem plans respect character level and do not guess missing requirements', () => {
  const gem = { skill_id: 'Synthetic', name: 'Synthetic', family: 'Synthetic', is_support: false, max_level: 3,
    level_requirements: [0, 10, 20, 20] };
  expect(usableGemLevel(gem, 9)).toBe(1);
  expect(usableGemLevel(gem, 10)).toBe(2);
  expect(usableGemLevel(gem, 20)).toBe(4);
  expect(usableGemLevel({ ...gem, level_requirements: undefined }, 100)).toBe(0);
});


test('ordinary supports are skill adjustments while lineage and active gems are market acquisitions', () => {
  const gem = { skill_id: 'Synthetic', name: 'Synthetic', family: 'Synthetic', is_support: true, max_level: 1 };
  expect(gemAcquisition(gem)).toBe('skill-adjustment');
  expect(gemAcquisition({ ...gem, is_lineage: true })).toBe('market-lineage');
  expect(gemAcquisition({ ...gem, is_support: false })).toBe('market-skill');
});

test('gem planning checks compatibility, current sockets, overlapping families and newly unsupported effects', async () => {
  const support = (id: string, extra = {}) => ({ skill_id: id, name: id, family: id, is_support: true,
    max_level: 1, level_requirements: [0], compatibility_known: true, require_skill_types: ['Spell'], ...extra });
  const catalog = { bases: [], mods: [], gems: [
    support('Fireball', { is_support: false, max_level: 20, skill_types: ['Spell'] }),
    support('Old', { families: ['A'] }), support('Ordinary', { families: ['A'] }),
    support('Lineage', { is_lineage: true }), support('Attack', { require_skill_types: ['Attack'] }),
    support('Unmodeled'), support('Unknown', { compatibility_known: false }),
  ] };
  const request = { character: { level: 50 }, socket_groups: [{ enabled: true,
    gems: [{ skill_id: 'Fireball', level: 1, quality: 20 }, { skill_id: 'Old', level: 1, quality: 20 }] }] };
  const evaluate = async ({ variants }: EvaluateOptions) => ({
    baseline: { TotalDPS: 100 }, aborted: false,
    results: variants.map((variant, index) => {
      const ids = variant.socket_groups?.[0].gems.map(gem => gem.skill_id) ?? ['Fireball', 'Old'];
      expect(ids).not.toContain('Attack');
      expect(ids).not.toContain('Unknown');
      expect(ids.includes('Old') && ids.includes('Ordinary')).toBe(false);
      expect(ids.length).toBeLessThanOrEqual(3);
      return { index, label: null, error: null,
        stats: { TotalDPS: ids.includes('Unmodeled') ? 500 : ids.includes('Lineage') ? 200 : ids.includes('Ordinary') ? 180 : 100 },
        unsupported: ['Existing Guard', ...(ids.includes('Unmodeled') ? ['New unmodeled effect'] : [])] };
    }),
  });
  const plans = await planGemUpgrades(request, catalog, 0, { stat: 'TotalDPS', constraints: [] }, undefined, undefined, evaluate);
  expect(plans.map(plan => plan.gem.skill_id)).toEqual(['Lineage', 'Ordinary']);
  expect(plans.map(plan => plan.acquisition)).toEqual(['market-lineage', 'skill-adjustment']);
  expect(plans[1].variant.socket_groups![0].gems.map(gem => gem.skill_id)).toEqual(['Fireball', 'Ordinary']);
});

test('level and quality upgrades preserve the current skill form without transferring it to other gems', () => {
  const request = { socket_groups: [{ enabled: true, gems: [{ skill_id: 'IceNovaPlayer', level: 19, quality: 0, stat_set_index: 2 }] }] };
  const gem = { skill_id: 'IceNovaPlayer', name: 'Ice Nova', family: 'IceNova', is_support: false, max_level: 20 };
  expect(gemVariant(request, 0, 0, gem, 20, 20).socket_groups![0].gems[0]).toEqual({ skill_id: 'IceNovaPlayer', level: 20, quality: 20, stat_set_index: 2 });
  expect(gemVariant(request, 0, 0, { ...gem, skill_id: 'FireballPlayer' }, 20, 20).socket_groups![0].gems[0].stat_set_index).toBeUndefined();
});
