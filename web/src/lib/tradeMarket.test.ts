import { expect, test, vi } from 'vitest';
import type { EvaluateOptions } from './optimize';
import { evaluateMarket, gemAcquisition, gemVariant, planGemUpgrades, rankMarket, usableGemLevel, type MarketResponse, type MarketUpgrade } from './tradeMarket';

const market: MarketResponse = { url: 'https://www.pathofexile.com/trade2/search/poe2/Standard/synthetic',
  total: 50, sampled: 3, listings: [
    { id: 'a', price: { amount: 90, currency: 'exalted' }, item: { name: 'High linear weight' } },
    { id: 'b', price: { amount: 10, currency: 'exalted' }, item: { name: 'Better combination' } },
    { id: 'c', price: { amount: 1, currency: 'exalted' }, item: { name: 'Broken item' } },
  ] };
test('market reranks whole-item synergy, retaining prices, negatives and unsupported lines', async () => {
  const evaluate = vi.fn(async (options: EvaluateOptions) => {
    expect(options.variants[1].set_items).toEqual([{ slot: 'weapon2', text: 'flat + speed' }]);
    return { baseline: { TotalDPS: 100, Life: 1000 }, aborted: false, results: [
      { index: 0, label: null, error: null, stats: { TotalDPS: 120, Life: 1000 } },
      { index: 1, label: null, error: null, stats: { TotalDPS: 180, Life: 950 }, unsupported: ['unmodeled'] },
    ] };
  });
  const result = await evaluateMarket({ request: {}, slot: 'weapon2', market,
    objective: { stat: 'TotalDPS', constraints: [] }, evaluate,
    importItems: async () => [{ text: 'flat' }, { text: 'flat + speed' }, { error: 'invalid' }],
  });
  expect(result.upgrades.map(entry => entry.listing.id)).toEqual(['b', 'a']);
  expect(result.upgrades[0].gain).toBe(80);
  expect(result.upgrades[0].stats.Life).toBe(950);
  expect(result.upgrades[0].warnings).toEqual(['unmodeled']);
  expect(result.rejected).toBe(1);
});
test('cancelled recalculation cannot publish a partially ranked market', async () => {
  await expect(evaluateMarket({ request: {}, slot: 'ring1', market,
    objective: { stat: 'Life', constraints: [] }, importItems: async () => [{ text: 'item' }],
    evaluate: async () => ({ baseline: {}, results: [], aborted: true }),
  })).rejects.toThrow('cancelled');
});
test('value sorting does not equate a divine with an exalted', () => {
  const entries = [
    { gain: 100, listing: { price: { amount: 1, currency: 'divine' } } },
    { gain: 50, listing: { price: { amount: 5, currency: 'exalted' } } },
    { gain: 100, listing: { price: { amount: 20, currency: 'exalted' } } },
  ] as MarketUpgrade[];
  expect(rankMarket(entries, 'exalted')).toEqual([entries[1], entries[2], entries[0]]);
});
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
