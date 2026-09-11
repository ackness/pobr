// Cloudflare Pages worker; Vite runs the same handler locally.
const API = 'https://www.wegame.com.cn/api/v1/wegame.pallas.poe2.Profile/';
const LIMIT = 4 * 1024 * 1024;

export function shareKey(input) {
  const url = new URL(input.trim());
  if (url.protocol !== 'https:' || url.hostname !== 'www.wegame.com.cn' ||
      url.port || url.username || url.password || url.pathname !== '/helper/poe2/') {
    throw new Error('Expected a WeGame PoE2 share URL.');
  }
  const match = /^#\/share\/([A-Za-z0-9_-]{16,256})\/?$/.exec(url.hash);
  if (!match) throw new Error('Invalid WeGame share code.');
  return match[1];
}

async function readJson(response, limit = LIMIT) {
  if (!response.body) throw new Error('Empty response.');
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let size = 0, text = '';
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > limit) throw new Error('Response is too large.');
      text += decoder.decode(value, { stream: true });
    }
    return JSON.parse(text + decoder.decode());
  } finally {
    await reader.cancel();
  }
}

// Keep calculation fields, excluding account identifiers, character names and URLs.
function itemFields(item, depth = 0) {
  if (!item || typeof item !== 'object' || depth > 8) throw new Error('Invalid WeGame item.');
  const keys = ['baseType', 'typeLine', 'name', 'frameType', 'ilvl', 'inventoryId', 'x',
    'properties', 'requirements', 'implicitMods', 'explicitMods', 'runeMods', 'enchantMods', 'craftedMods',
    'fracturedMods', 'corrupted', 'support', 'socket', 'sockets'];
  const result = Object.fromEntries(keys.filter(k => k in item).map(k => [k, item[k]]));
  if (item.socketedItems) result.socketedItems = item.socketedItems.map(i => itemFields(i, depth + 1));
  return result;
}

export async function fetchShare(url, fetcher = fetch) {
  const code = shareKey(url);
  const methods = ['GetRoleInfo', 'GetEquipments', 'GetTalentTree', 'GetJewels', 'GetSkills'];
  const responses = await Promise.all(methods.map(async method => {
    const response = await fetcher(API + method, {
      // Workers supports manual redirects; the non-2xx check below rejects them.
      method: 'POST', redirect: 'manual', credentials: 'omit',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ share_code: code, area: 0, from_src: 'poe2_helper' }),
      signal: AbortSignal.timeout(15000),
    });
    if (!response.ok) throw new Error(`WeGame ${method}: HTTP ${response.status}.`);
    const data = await readJson(response);
    if (data.result?.error_code !== 0) {
      throw new Error(`WeGame ${method} failed; the share may have expired or been made private.`);
    }
    return data;
  }));
  const [role, equipment, tree, jewels, skills] = responses;
  if (!role.role || !Array.isArray(equipment.equipments) || !tree.talent_tree ||
      !Array.isArray(skills.skills) || typeof jewels.jewel_data !== 'string') {
    throw new Error('Incomplete WeGame share response.');
  }
  return {
    format: 'wegame', version: 1,
    role: { level: role.role.level, class_name: role.role.class_name },
    equipments: equipment.equipments.map(i => itemFields(i)),
    talent_tree: tree.talent_tree,
    jewel_data: jewels.jewel_data,
    skills: skills.skills.map(i => itemFields(i)),
  };
}

export async function fetchTradeLeagues(realm, fetcher = fetch) {
  const hosts = { intl: 'https://www.pathofexile.com', cn: 'https://poe.game.qq.com' };
  if (!Object.hasOwn(hosts, realm)) throw new Error('Invalid trade realm.');
  const response = await fetcher(`${hosts[realm]}/api/trade2/data/leagues`, {
    redirect: 'manual', credentials: 'omit', signal: AbortSignal.timeout(10000),
    headers: { accept: 'application/json' },
  });
  if (!response.ok) throw new Error(`Trade leagues: HTTP ${response.status}.`);
  const data = await readJson(response, 256 * 1024);
  if (!Array.isArray(data.result)) throw new Error('Invalid league response.');
  const leagues = [...new Set(data.result.filter(entry =>
    (!entry.realm || entry.realm === 'poe2') && typeof entry.id === 'string' &&
    entry.id.length > 0 && entry.id.length <= 128,
  ).map(entry => entry.id))];
  if (leagues.length === 0) throw new Error('No trade leagues returned.');
  return { leagues };
}

const TRADE_HOSTS = { intl: 'https://www.pathofexile.com', cn: 'https://poe.game.qq.com' };
const TRADE_CATEGORIES = new Set(['accessory.amulet', 'accessory.ring', 'accessory.belt',
  'armour.chest', 'armour.helmet', 'armour.gloves', 'armour.boots', 'armour.quiver',
  'armour.shield', 'armour.focus', 'armour.buckler', 'weapon.bow', 'weapon.crossbow',
  'weapon.staff', 'weapon.warstaff', 'weapon.talisman', 'weapon.onemace', 'weapon.twomace',
  'weapon.wand', 'weapon.sceptre', 'weapon.spear', 'weapon.flail', 'jewel', 'flask',
  'flask.life', 'flask.mana', 'flask.charm', 'gem']);

export async function fetchTradeMarket(input, fetcher = fetch) {
  if (!input || typeof input !== 'object') throw Object.assign(new Error('Invalid trade search.'), { status: 400 });
  const { realm, league, category, weighted = [], price, gem, maxLevel } = input;
  if (!Object.hasOwn(TRADE_HOSTS, realm) || typeof league !== 'string' || !league.trim() ||
      league.length > 128 || !TRADE_CATEGORIES.has(category) || !Array.isArray(weighted) || weighted.length > 32 ||
      weighted.some(stat => !stat || !/^explicit\.stat_\d+$/.test(stat.id) || !Number.isFinite(stat.weight)) ||
      (price && (!Number.isFinite(price.max) || price.max <= 0 ||
        (price.currency && !['divine', 'exalted', 'chaos'].includes(price.currency)))) ||
      (maxLevel !== undefined && (!Number.isInteger(maxLevel) || maxLevel < 1 || maxLevel > 100)) ||
      (gem && (category !== 'gem' || typeof gem.name !== 'string' || gem.name.length > 100 ||
        !Number.isInteger(gem.level) || gem.level < 1 || gem.level > 21 ||
        !Number.isInteger(gem.quality) || gem.quality < 0 || gem.quality > 23))) {
    throw Object.assign(new Error('Invalid trade search.'), { status: 400 });
  }
  const host = TRADE_HOSTS[realm];
  const query = {
    query: {
      status: { option: 'online' },
      ...(gem ? { type: gem.name } : {}),
      stats: weighted.length ? [{ type: 'weight', filters: weighted.map(stat => ({ id: stat.id, value: { weight: stat.weight } })) }]
        : [{ type: 'and', filters: [] }],
      filters: {
        type_filters: { filters: { category: { option: category }, ...(gem ? { quality: { min: gem.quality } } : {}) } },
        ...(price ? { trade_filters: { filters: { price: { max: price.max, ...(price.currency ? { option: price.currency } : {}) } } } } : {}),
        ...(maxLevel ? { req_filters: { filters: { lvl: { max: maxLevel } } } } : {}),
        ...(gem ? { misc_filters: { filters: { gem_level: { min: gem.level } } } } : {}),
      },
    },
    sort: weighted.length ? { 'statgroup.0': 'desc' } : { price: 'asc' },
  };
  const upstream = async (path, init = {}) => {
    const response = await fetcher(host + path, { ...init, redirect: 'manual', credentials: 'omit',
      signal: AbortSignal.timeout(15000), headers: { 'content-type': 'application/json',
        'user-agent': 'PoBR (+https://github.com/ackness/pobr)', ...init.headers } });
    if (!response.ok) {
      const detail = await readJson(response, 256 * 1024).catch(() => null);
      const message = response.status === 429 ? 'Trade rate limit reached. Please wait before trying again.'
        : [401, 403].includes(response.status) ? 'Trade access requires verification. Open the official trade site to continue.'
        : `Trade service returned HTTP ${response.status}.`;
      throw Object.assign(new Error(message), { status: response.status, upstream_code: detail?.error?.code, retry_after: response.headers.get('retry-after') });
    }
    return readJson(response);
  };
  const path = `/api/trade2/search/poe2/${encodeURIComponent(league)}`;
  let search, search_mode = weighted.length ? 'weighted' : 'price';
  try {
    search = await upstream(path, { method: 'POST', body: JSON.stringify(query) });
  } catch (error) {
    if (error.status !== 400 || error.upstream_code !== 2 || !weighted.length) throw error;
    // Anonymous trade has a much smaller complexity limit. One simpler query is
    // allowed by that response; never retry authentication failures or rate limits.
    const simple = structuredClone(query);
    simple.query.stats = [{ type: 'and', filters: [] }];
    delete simple.query.filters.req_filters;
    simple.sort = { price: price ? 'desc' : 'asc' };
    search = await upstream(path, { method: 'POST', body: JSON.stringify(simple) });
    search_mode = 'budget';
  }
  if (typeof search.id !== 'string' || !/^[\w-]+$/.test(search.id) || !Array.isArray(search.result)) throw new Error('Invalid trade result.');
  const ids = [...new Set(search.result)].filter(id => typeof id === 'string' && /^[a-f0-9]{64}$/.test(id)).slice(0, 20);
  const listings = [];
  for (let start = 0; start < ids.length; start += 10) {
    const data = await upstream(`/api/trade2/fetch/${ids.slice(start, start + 10).join(',')}?query=${encodeURIComponent(search.id)}`);
    for (const entry of data.result ?? []) {
      const price = entry?.listing?.price;
      if (!entry?.item || !price || !Number.isFinite(price.amount) || !ids.includes(entry.id)) continue;
      const requiredLevel = entry.item.requirements?.find(req => /^(Level|等级|等級)$/.test(req.name))?.values?.[0]?.[0];
      if (maxLevel && Number.parseInt(requiredLevel, 10) > maxLevel) continue;
      // Like PoB2's Buy action, narrow the link to this seller, base/name and exact price.
      // Keep account/whisper objects out of the response; the account filter is only in the buying URL.
      const exact = structuredClone(query);
      const account = entry.listing.account?.name;
      if (typeof account === 'string') {
        exact.query.type = entry.item.baseType || entry.item.typeLine;
        if (entry.item.name) exact.query.name = entry.item.name;
        exact.query.filters.trade_filters = { filters: { account: { input: account },
          price: { min: price.amount, max: price.amount, option: price.currency } } };
      }
      const url = typeof account === 'string'
        ? `${host}/trade2/search/poe2/${encodeURIComponent(league)}?q=${encodeURIComponent(JSON.stringify(exact))}`
        : `${host}/trade2/search/poe2/${encodeURIComponent(league)}/${encodeURIComponent(search.id)}`;
      listings.push({ id: entry.id, url, price: { amount: price.amount, currency: price.currency }, item: itemFields(entry.item) });
    }
  }
  return { url: `${host}/trade2/search/poe2/${encodeURIComponent(league)}/${encodeURIComponent(search.id)}`,
    total: search.total, sampled: ids.length, search_mode, listings };
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname === '/api/trade/search') {
      if (request.method !== 'POST') return new Response('Method not allowed', { status: 405 });
      const origin = request.headers.get('origin');
      if (origin && origin !== url.origin) return new Response('Forbidden', { status: 403 });
      try {
        return Response.json(await fetchTradeMarket(await readJson(request, 32768)), { headers: { 'cache-control': 'no-store' } });
      } catch (error) {
        return Response.json({ error: error.message, retry_after: error.retry_after }, {
          status: [400, 401, 403, 429].includes(error.status) ? error.status : 502,
          headers: { 'cache-control': 'no-store' },
        });
      }
    }
    if (url.pathname === '/api/trade/leagues') {
      if (request.method !== 'GET') return new Response('Method not allowed', { status: 405 });
      const realm = url.searchParams.get('realm') ?? 'intl';
      if (!['intl', 'cn'].includes(realm)) return new Response('Invalid realm', { status: 400 });
      try {
        return Response.json(await fetchTradeLeagues(realm), { headers: { 'cache-control': 'public, max-age=600' } });
      } catch {
        return Response.json({ error: 'Trade league list is temporarily unavailable.' }, { status: 502 });
      }
    }
    if (url.pathname !== '/api/import/wegame') return env.ASSETS.fetch(request);
    const headers = { 'cache-control': 'no-store' };
    if (request.method !== 'POST') return new Response('Method not allowed', { status: 405, headers: { ...headers, allow: 'POST' } });
    const origin = request.headers.get('origin');
    if (origin && origin !== url.origin) return new Response('Forbidden', { status: 403, headers });
    let input;
    try {
      input = (await readJson(request, 2048)).url;
      shareKey(input);
    } catch {
      return Response.json({ error: 'Invalid WeGame PoE2 share URL.' }, { status: 400, headers });
    }
    try {
      return Response.json(await fetchShare(input), { headers });
    } catch (error) {
      return Response.json({ error: error instanceof Error ? error.message : 'WeGame import failed.' }, { status: 502, headers });
    }
  },
};
