import { expect, test } from 'vitest';
import { readTradeExport, tradeExportBookmark } from './tradeExport';
import { buildTradeUrl } from './trade';

const query = { realm: 'cn' as const, league: 'Test League', category: 'accessory.amulet', price: { max: 50, currency: 'exalted' as const }, maxLevel: 68 };
const url = 'https://poe.game.qq.com/trade2/search/poe2/Test%20League/synthetic';
const fixture = () => ({ format: 'pobr-trade', version: 1, realm: 'cn', league: 'Test League', category: 'accessory.amulet', market: {
  url, total: 3, sampled: 3, listings: [
    { id: 'a', price: { amount: 10, currency: 'exalted' }, item: { baseType: 'Amber Amulet' } },
    { id: 'b', price: { amount: 60, currency: 'exalted' }, item: { baseType: 'Amber Amulet' } },
    { id: 'c', price: { amount: 1, currency: 'divine' }, item: { baseType: 'Amber Amulet' } },
  ],
} });
test('import reapplies current budget and does not equate currencies', () => {
  const market = readTradeExport(JSON.stringify(fixture()), query);
  expect(market.listings.map(row => row.id)).toEqual(['a']);
});
test('rejects exports from the wrong realm, league, category and non-official links', () => {
  for (const changed of [{ realm: 'intl' }, { league: 'Other' }, { category: 'jewel' }]) {
    expect(() => readTradeExport(JSON.stringify({ ...fixture(), ...changed }), query)).toThrow('does not match');
  }
  const value = fixture(); value.market.url = 'javascript:alert(1)';
  expect(() => readTradeExport(JSON.stringify(value), query)).toThrow('trade URL');
});
test('bookmark captures the selected category and budget without login data', () => {
  const bookmark = tradeExportBookmark(buildTradeUrl(query.league, [], query), query);
  expect(bookmark).toContain('javascript:void(');
  expect(bookmark).toContain('"accessory.amulet"');
  expect(bookmark).toContain('"max":50');
  expect(bookmark).not.toContain('document.cookie');
});
