import { buildTradeUrl } from './trade';
import type { MarketQuery, MarketResponse } from './tradeMarket';

/** Runs only when the player activates the bookmark on the official trade host.
 * It uses that tab's normal authenticated fetch and never reads or exports cookies/tokens.
 * Keep this function self-contained: its compiled source is used as a bookmark URL.
 */
async function exportOfficialCandidates(input: {
  origin: string; league: string; category: string; realm: string; query: object; gemName?: string;
}) {
  try {
    if (location.origin !== input.origin) throw new Error('Open the official trade site and sign in first.');
    const get = async (path: string, body?: object) => {
      const response = await fetch(input.origin + path, { method: body ? 'POST' : 'GET',
        credentials: 'same-origin', redirect: 'error', signal: AbortSignal.timeout(15000),
        headers: { 'content-type': 'application/json' }, ...(body ? { body: JSON.stringify(body) } : {}) });
      if (!response.ok) throw new Error(`Trade HTTP ${response.status}. Please sign in or wait before trying again.`);
      return response.json();
    };
    const search = await get(`/api/trade2/search/poe2/${encodeURIComponent(input.league)}`, input.query);
    if (typeof search.id !== 'string' || !/^[\w-]+$/.test(search.id) || !Array.isArray(search.result)) throw new Error('Invalid search response.');
    const ids = [...new Set<string>(search.result)].filter(id => /^[a-f0-9]{64}$/.test(id)).slice(0, 20);
    const fields = ['baseType', 'typeLine', 'name', 'frameType', 'ilvl', 'properties', 'requirements',
      'implicitMods', 'explicitMods', 'runeMods', 'enchantMods', 'craftedMods', 'fracturedMods',
      'corrupted', 'support', 'socket', 'sockets'];
    const project = (item: Record<string, unknown>, depth = 0): Record<string, unknown> => {
      if (!item || depth > 8) throw new Error('Invalid item.');
      const out: Record<string, unknown> = Object.fromEntries(fields.filter(key => key in item).map(key => [key, item[key]]));
      if (Array.isArray(item.socketedItems)) out.socketedItems = item.socketedItems.map(entry => project(entry, depth + 1));
      return out;
    };
    const url = `${input.origin}/trade2/search/poe2/${encodeURIComponent(input.league)}/${search.id}`;
    const listings = [];
    for (let start = 0; start < ids.length; start += 10) {
      const response = await get(`/api/trade2/fetch/${ids.slice(start, start + 10).join(',')}?query=${search.id}`);
      for (const entry of response.result ?? []) {
        const price = entry?.listing?.price;
        if (!ids.includes(entry?.id) || !entry.item || !price || !Number.isFinite(price.amount)) continue;
        const exact = { query: { status: { option: 'any' }, type: entry.item.baseType || entry.item.typeLine,
          ...(entry.item.name ? { name: entry.item.name } : {}), stats: [{ type: 'and', filters: [] }],
          filters: { type_filters: { filters: { category: { option: input.category } } },
            trade_filters: { filters: { account: { input: entry.listing.account?.name },
              price: { min: price.amount, max: price.amount, option: price.currency } } } } }, sort: { price: 'asc' } };
        const buy = entry.listing.account?.name
          ? `${input.origin}/trade2/search/poe2/${encodeURIComponent(input.league)}?q=${encodeURIComponent(JSON.stringify(exact))}` : url;
        listings.push({ id: entry.id, url: buy, price: { amount: price.amount, currency: price.currency }, item: project(entry.item) });
      }
    }
    const blob = new Blob([JSON.stringify({ format: 'pobr-trade', version: 1, realm: input.realm,
      league: input.league, category: input.category, gem_name: input.gemName, exported_at: new Date().toISOString(),
      market: { url, total: search.total, sampled: ids.length, listings } })], { type: 'application/json' });
    const href = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = href; anchor.download = `pobr-trade-${input.category}.json`;
    document.body.append(anchor); anchor.click(); anchor.remove();
    setTimeout(() => URL.revokeObjectURL(href), 1000);
    alert(`PoBR: exported ${listings.length} items. Return to PoBR and import this file.\n已导出商品，请回到 PoBR 导入此文件。`);
  } catch (error) {
    alert(error instanceof Error ? error.message : String(error));
  }
}

export function tradeExportBookmark(searchUrl: string, market: MarketQuery): string {
  const url = new URL(searchUrl);
  if (url.origin !== (market.realm === 'cn' ? 'https://poe.game.qq.com' : 'https://www.pathofexile.com')) throw new Error('Invalid trade host.');
  const query = JSON.parse(url.searchParams.get('q') ?? '{}');
  delete query.engine;
  query.query.filters.req_filters = { filters: { lvl: { max: market.maxLevel ?? 100 } } };
  const input = { origin: url.origin, league: market.league, category: market.category, realm: market.realm, gemName: market.gem?.name, query };
  return `javascript:void(${exportOfficialCandidates.toString()}(${JSON.stringify(input)}))`;
}

/** Accept only a matching realm/league/category and official buying URLs.
 * Apply the current budget again so an older export cannot recommend an over-budget listing.
 */
export function readTradeExport(text: string, expected: MarketQuery): MarketResponse {
  if (text.length > 4 * 1024 * 1024) throw new Error('Trade export is too large.');
  const data = JSON.parse(text);
  if (data.format !== 'pobr-trade' || data.version !== 1 || data.realm !== expected.realm ||
      data.league !== expected.league || data.category !== expected.category ||
      (expected.gem && data.gem_name !== expected.gem.name)) {
    throw new Error('Trade export server, league or item type does not match this search.');
  }
  const market = data.market as MarketResponse;
  if (!market || !Array.isArray(market.listings) || market.listings.length > 100) throw new Error('Invalid trade export.');
  const origin = expected.realm === 'cn' ? 'https://poe.game.qq.com' : 'https://www.pathofexile.com';
  const validUrl = (raw: string) => {
    const url = new URL(raw);
    return url.origin === origin && !url.username && !url.password &&
      (url.pathname === `/trade2/search/poe2/${encodeURIComponent(expected.league)}` ||
        url.pathname.startsWith(`/trade2/search/poe2/${encodeURIComponent(expected.league)}/`));
  };
  if (!validUrl(market.url)) throw new Error('Invalid trade URL.');
  const listings = market.listings.filter(entry => {
    if (!entry || typeof entry.id !== 'string' || !entry.item || !entry.price ||
        !Number.isFinite(entry.price.amount) || entry.price.amount < 0 || typeof entry.price.currency !== 'string' ||
        (entry.url && !validUrl(entry.url))) throw new Error('Invalid trade listing.');
    const requiredLevel = entry.item.requirements?.find(req => /^(Level|等级|等級)$/.test(req.name))?.values[0]?.[0];
    if (expected.maxLevel && Number.parseInt(requiredLevel ?? '', 10) > expected.maxLevel) return false;
    if (!expected.price) return true;
    return entry.price.currency === (expected.price.currency ?? 'exalted') && entry.price.amount <= expected.price.max;
  });
  return { ...market, search_mode: 'weighted', sampled: listings.length,
    total: Number.isFinite(market.total) ? market.total : listings.length, listings };
}

/** The same exact gem query can be opened in the signed-in official browser. */
export function gemTradeUrl(input: MarketQuery): string {
  if (!input.gem) throw new Error('Choose a gem candidate.');
  const url = new URL(buildTradeUrl(input.league, [], { realm: input.realm, category: 'gem', price: input.price }));
  const query = JSON.parse(url.searchParams.get('q')!);
  query.query.type = input.gem.name;
  query.query.stats = [{ type: 'and', filters: [] }];
  query.query.filters.type_filters.filters.quality = { min: input.gem.quality };
  query.query.filters.misc_filters = { filters: { gem_level: { min: input.gem.level } } };
  query.sort = { price: 'asc' };
  url.searchParams.set('q', JSON.stringify(query));
  return url.toString();
}
