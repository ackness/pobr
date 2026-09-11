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

/**
 * 预算上限（trade2 Buyout Price 过滤器）。currency 取官方过滤器 id：
 * 缺省（undefined）= Exalted Orb Equivalent（崇高石等价，站方自动换算）。
 */
export interface TradePriceCap {
  max: number;
  currency?: 'divine' | 'exalted' | 'chaos';
}

/** Category is mandatory: an untyped weighted query mixes unrelated equipment. */
export function buildTradeUrl(
  league: string,
  weighted: WeightedStat[],
  options: { category: string; realm?: TradeRealm; price?: TradePriceCap; minimumWeight?: number },
): string {
  const { category, realm = 'intl', price, minimumWeight = 0 } = options;
  if (!category) throw new Error('Select an item category before searching');
  const query = {
    query: {
      status: { option: 'online' },
      stats: [
        {
          type: 'weight',
          value: { min: Math.round(minimumWeight * 1000) / 1000 },
          filters: weighted.map((w) => ({
            id: w.id,
            value: { weight: Math.round(w.weight * 1000) / 1000 },
          })),
        },
      ],
      filters: {
        type_filters: { filters: {
          category: { option: category },
        } },
        ...(price && price.max > 0
          ? {
              trade_filters: {
                filters: {
                  price: { max: price.max, ...(price.currency ? { option: price.currency } : {}) },
                },
              },
            }
          : {}),
      },
    },
    sort: { 'statgroup.0': 'desc' },
    engine: 'new',
  };
  return `${REALM_HOSTS[realm]}/trade2/search/poe2/${encodeURIComponent(
    league,
  )}?q=${encodeURIComponent(JSON.stringify(query))}`;
}
