/** Official trade templates, live realm/league discovery and weighted search links.
 * Affix probes live in tradeOptimizer.ts; actual listing recalculation lives in tradeMarket.ts.
 */

import type { TradeStatEntry } from '../api/types';

interface TradeMapFile {
  templates: Record<string, TradeStatEntry>;
}

let mapPromise: Promise<Record<string, TradeStatEntry>> | null = null;

/** 数字骨架化（与 pipeline/extract-trade-map.lua 同规则）：先剥 {…} 标注再归一数字。 */
export function normalizeTradeLine(line: string): string {
  return line
    .replace(/\{[^}]*\}/g, '')
    .trim()
    .replace(/\d+(\.\d+)?/g, '#');
}

/** 加载词条→trade id 映射（静态资产；缺文件/mock 后端返回空表 → 功能隐藏）。 */
export function loadTradeMap(): Promise<Record<string, TradeStatEntry>> {
  mapPromise ??= (async () => {
    try {
      const manifest = (await (await fetch('/data/manifest.json')).json()) as {
        version: string;
      };
      const res = await fetch(`/data/${manifest.version}/overlay/trade_stat_map.json`);
      if (!res.ok) return {};
      return ((await res.json()) as TradeMapFile).templates ?? {};
    } catch {
      return {};
    }
  })();
  return mapPromise;
}

/** Representative stat value: average flat damage range, otherwise the signed number. */
export function lineValue(line: string): number | null {
  const clean = line.replace(/\{[^}]*\}/g, '');
  const range = clean.match(/(-?\d+(?:\.\d+)?) to (-?\d+(?:\.\d+)?)/);
  if (range) return (Number(range[1]) + Number(range[2])) / 2;
  const m = clean.match(/-?\d+(\.\d+)?/);
  return m ? Number(m[0]) : null;
}

export interface WeightedStat {
  /** 官方 trade stat id（explicit.stat_N）。 */
  id: string;
  /** 每单位词条数值的目标收益（trade2 weight 系数）。 */
  weight: number;
  /** 原词条行（展示用）。 */
  line: string;
  /** 当前数值（加权和门槛用）。 */
  value: number;
}

/** Keep the displayed reference and the official query on exactly the same scale. */
export function tradeQueryWeights<T extends WeightedStat>(weighted: readonly T[]): T[] {
  const seen = new Set<string>();
  return weighted.flatMap(stat => {
    const weight = Math.round(stat.weight * 1000) / 1000;
    if (!Number.isFinite(weight) || weight === 0 || seen.has(stat.id)) return [];
    seen.add(stat.id);
    return [{ ...stat, weight }];
  }).slice(0, 32);
}

/** 服务器：国际服 / 国服（腾讯）。路径结构相同，仅主机名不同；stat id 通用。 */
export type TradeRealm = 'intl' | 'cn';

const REALM_HOSTS: Record<TradeRealm, string> = {
  intl: 'https://www.pathofexile.com',
  cn: 'https://poe.game.qq.com',
};

/** Offline fallback; the UI refreshes these from the official realm-specific API. */
export const REALM_LEAGUES: Record<TradeRealm, string[]> = {
  intl: ['Forbidden Rites', 'HC Forbidden Rites', 'Runes of Aldur', 'HC Runes of Aldur', 'Standard', 'Hardcore'],
  cn: ['周年庆巅峰挑战', '奥杜尔秘符', '标准'],
};

export const REALM_DEFAULT_LEAGUE: Record<TradeRealm, string> = {
  intl: REALM_LEAGUES.intl[0],
  cn: REALM_LEAGUES.cn[0],
};

/** Live official lists; fallback values remain usable during upstream outages. */
export async function loadTradeLeagues(realm: TradeRealm): Promise<string[]> {
  const response = await fetch(`/api/trade/leagues?realm=${realm}`);
  if (!response.ok) throw new Error('League list unavailable');
  const data = await response.json() as { leagues?: unknown };
  if (!Array.isArray(data.leagues) || data.leagues.length === 0 ||
      !data.leagues.every(value => typeof value === 'string' && value.length > 0)) {
    throw new Error('Invalid league list');
  }
  return data.leagues;
}

/** Price options shared by both realms; "equiv" omits the official currency filter. */
export const TRADE_CURRENCIES = [
  { value: 'equiv', labelKey: 'trade.curExaltedEquivalent' },
  { value: 'exalted_divine', labelKey: 'trade.curExaltedDivine' },
  { value: 'exalted', labelKey: 'trade.curExalted' },
  { value: 'divine', labelKey: 'trade.curDivine' },
  { value: 'chaos', labelKey: 'trade.curChaos' },
  { value: 'regal', labelKey: 'trade.curRegal' },
  { value: 'alch', labelKey: 'trade.curAlchemy' },
  { value: 'vaal', labelKey: 'trade.curVaal' },
  { value: 'annul', labelKey: 'trade.curAnnulment' },
  { value: 'aug', labelKey: 'trade.curAugmentation' },
  { value: 'transmute', labelKey: 'trade.curTransmutation' },
  { value: 'mirror', labelKey: 'trade.curMirror' },
] as const;

export type TradeCurrency = typeof TRADE_CURRENCIES[number]['value'];

/** Buyout price cap; "equiv" or an omitted currency uses Exalted Orb Equivalent. */
export interface TradePriceCap {
  max: number;
  currency?: TradeCurrency;
}

export interface TradeQueryOptions {
  category: string;
  realm?: TradeRealm;
  price?: TradePriceCap;
  minimumWeight?: number;
  maxLevel?: number;
  includeUnique?: boolean;
  requiredStats?: string[];
}

/** CN's instant-buy market includes listings whose owners are offline. */
export function buildTradeQuery(weighted: WeightedStat[], options: TradeQueryOptions) {
  const { category, realm = 'intl', price, maxLevel } = options;
  const weights = tradeQueryWeights(weighted);
  if (!category) throw new Error('Select an item category before searching');
  return {
    query: {
      status: { option: realm === 'cn' ? 'any' : 'online' },
      stats: [...(weights.length ? [{
        type: 'weight',
        ...(options.minimumWeight && options.minimumWeight > 0 ? { value: { min: Math.round(options.minimumWeight * 1000) / 1000 } } : {}),
        filters: weights.map(w => ({ id: w.id, value: { weight: w.weight } })),
      }] : [{ type: 'and', filters: [] }]), ...(options.requiredStats?.length ? [{ type: 'and', filters: [...new Set(options.requiredStats)].map(id => ({ id })) }] : [])],
      filters: {
        type_filters: { filters: {
          category: { option: category },
          ...(!options.includeUnique && !category.startsWith('gem') ? { rarity: { option: 'nonunique' } } : {}),
        } },
        ...(price && price.max > 0 ? { trade_filters: { filters: {
          price: { max: price.max, ...(price.currency && price.currency !== 'equiv' ? { option: price.currency } : {}) },
        } } } : {}),
        ...(maxLevel ? { req_filters: { filters: { lvl: { max: maxLevel } } } } : {}),
      },
    },
    sort: weights.length ? { 'statgroup.0': 'desc' } : { price: 'asc' },
  };
}

/** Category is mandatory: an untyped weighted query mixes unrelated equipment. */
export function buildTradeUrl(
  league: string,
  weighted: WeightedStat[],
  options: TradeQueryOptions,
): string {
  const { realm = 'intl' } = options;
  const query = { ...buildTradeQuery(weighted, options), engine: 'new' };
  return `${REALM_HOSTS[realm]}/trade2/search/poe2/${encodeURIComponent(
    league,
  )}?q=${encodeURIComponent(JSON.stringify(query))}`;
}

/** Exact gem level and quality filters for a locally evaluated upgrade plan. */
export function gemTradeUrl(input: TradeQueryOptions & { league: string; gem: { name: string; level: number; quality: number } }): string {
  const url = new URL(buildTradeUrl(input.league, [], input));
  const query = JSON.parse(url.searchParams.get('q')!);
  query.query.type = input.gem.name;
  query.query.filters.type_filters.filters.quality = { min: input.gem.quality };
  query.query.filters.misc_filters = { filters: { gem_level: { min: input.gem.level } } };
  url.searchParams.set('q', JSON.stringify(query));
  return url.toString();
}
