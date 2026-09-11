import { afterEach, describe, expect, test, vi } from 'vitest';
import { buildTradeUrl, lineValue, normalizeTradeLine, loadTradeLeagues } from './trade';

describe('normalizeTradeLine', () => {
  test('skeletonizes numbers and strips annotations', () => {
    expect(normalizeTradeLine('+37 to Strength')).toBe('+# to Strength');
    expect(normalizeTradeLine('Adds 2 to 5 Physical Damage')).toBe('Adds # to # Physical Damage');
    expect(normalizeTradeLine('{tags:life}12% increased maximum Life')).toBe(
      '#% increased maximum Life',
    );
  });
});

describe('lineValue', () => {
  test('first number, null for flag lines', () => {
    expect(lineValue('+37 to Strength')).toBe(37);
    expect(lineValue('12.5% increased Attack Speed')).toBe(12.5);
    expect(lineValue('Cannot be Frozen')).toBeNull();
    expect(lineValue('Adds 2 to 10 Physical Damage')).toBe(6);
    expect(lineValue('-10% to Fire Resistance')).toBe(-10);
  });
});

describe('buildTradeUrl', () => {
  test('weighted query with threshold min and league in path', () => {
    const url = buildTradeUrl('Standard', [
      { id: 'explicit.stat_1', weight: 2, line: 'x', value: 10 },
      { id: 'explicit.stat_2', weight: 1, line: 'y', value: 5 },
    ], { category: 'armour.quiver', minimumWeight: 17.5 });
    expect(url.startsWith('https://www.pathofexile.com/trade2/search/poe2/Standard?q=')).toBe(
      true,
    );
    const q = JSON.parse(decodeURIComponent(url.split('?q=')[1]));
    expect(q.query.stats[0].type).toBe('weight');
    expect(q.query.filters.type_filters.filters.category.option).toBe('armour.quiver');
    expect(q.query.stats[0].value.min).toBe(17.5);
    expect(q.query.stats[0].filters).toHaveLength(2);
    expect(q.sort['statgroup.0']).toBe('desc');
  });

  test('price cap lands in trade_filters; currency omitted = exalted equivalent', () => {
    const weighted = [{ id: 'explicit.stat_1', weight: 1, line: 'x', value: 1 }];
    const capped = buildTradeUrl('Standard', weighted, { category: 'armour.quiver', price: { max: 100, currency: 'divine' } });
    const q = JSON.parse(decodeURIComponent(capped.split('?q=')[1]));
    expect(q.query.filters.trade_filters.filters.price).toEqual({ max: 100, option: 'divine' });

    const equiv = buildTradeUrl('Standard', weighted, { category: 'armour.quiver', price: { max: 50 } });
    const q2 = JSON.parse(decodeURIComponent(equiv.split('?q=')[1]));
    expect(q2.query.filters.trade_filters.filters.price).toEqual({ max: 50 });

    const uncapped = buildTradeUrl('Standard', weighted, { category: 'armour.quiver' });
    const q3 = JSON.parse(decodeURIComponent(uncapped.split('?q=')[1]));
    expect(q3.query.filters.trade_filters).toBeUndefined();
  });

  test('cn realm uses the Tencent host with a Chinese league name', () => {
    const url = buildTradeUrl(
      '深渊崛起',
      [{ id: 'explicit.stat_1', weight: 1, line: 'x', value: 1 }],
      { category: 'armour.quiver', realm: 'cn' },
    );
    expect(
      url.startsWith(
        `https://poe.game.qq.com/trade2/search/poe2/${encodeURIComponent('深渊崛起')}?q=`,
      ),
    ).toBe(true);
  });
});

afterEach(() => vi.unstubAllGlobals());

test('loads future official leagues instead of freezing the season list', async () => {
  vi.stubGlobal('fetch', vi.fn(async () => Response.json({ leagues: ['Future League', 'Standard'] })));
  expect(await loadTradeLeagues('intl')).toEqual(['Future League', 'Standard']);
  expect(fetch).toHaveBeenCalledWith('/api/trade/leagues?realm=intl');
});

test('upstream failure remains visible to the fallback UI', async () => {
  vi.stubGlobal('fetch', vi.fn(async () => new Response(null, { status: 502 })));
  await expect(loadTradeLeagues('cn')).rejects.toThrow();
  expect(() => buildTradeUrl('Standard', [], { category: '' })).toThrow('item category');
});
