/**
 * Trade 市集加权搜索（PoB2 TradeQueryGenerator 的最小复刻）：
 * - 词条 → 官方 trade2 stat id：`overlay/trade_stat_map.json`（vendor ModItem
 *   tradeHashes × TradeSiteStats 抽取，模板 = 数字骨架化词条行）；
 * - 权重 = 该词条的边际收益 ÷ 词条数值（摘一行重算一次，走通用变体框架——
 *   与 PoB2 的有限差分口径一致）；
 * - 产物 = `pathofexile.com/trade2/search/poe2/<league>?q=<json>` 直开 URL，
 *   网站前端自己读 q 预填查询，无需 OAuth。
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

/** 词条行的代表数值（权重分母）：首个数字；无数字（纯 flag 词条）为 null。 */
export function lineValue(line: string): number | null {
  const m = line.replace(/\{[^}]*\}/g, '').match(/\d+(\.\d+)?/);
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

/**
 * 各服的赛季预设（首项 = 当前挑战赛季，次项 = 常驻标准服）。
 * 赛季名每个版本会变（0.5 = Runes of Aldur / 奥杜尔秘符），随数据版本升级
 * 一并更新；过期期间用户可走「自定义」手填，功能不至于坏。
 */
export const REALM_LEAGUES: Record<TradeRealm, string[]> = {
  intl: ['Runes of Aldur', 'Standard'],
  cn: ['奥杜尔秘符', '标准'],
};

export const REALM_DEFAULT_LEAGUE: Record<TradeRealm, string> = {
  intl: REALM_LEAGUES.intl[0],
  cn: REALM_LEAGUES.cn[0],
};

/**
 * 预算上限（trade2 Buyout Price 过滤器）。currency 取官方过滤器 id：
 * 缺省（undefined）= Exalted Orb Equivalent（崇高石等价，站方自动换算）。
 */
export interface TradePriceCap {
  max: number;
  currency?: 'divine' | 'exalted' | 'chaos';
}

/** trade2 加权查询 JSON + 直开 URL（min = 当前加权和 × threshold）。 */
export function buildTradeUrl(
  league: string,
  weighted: WeightedStat[],
  threshold = 0.7,
  realm: TradeRealm = 'intl',
  price?: TradePriceCap,
): string {
  const sum = weighted.reduce((acc, w) => acc + w.weight * w.value, 0);
  const query = {
    query: {
      status: { option: 'any' },
      stats: [
        {
          type: 'weight',
          value: { min: Math.round(sum * threshold * 1000) / 1000 },
          filters: weighted.map((w) => ({
            id: w.id,
            value: { weight: Math.round(w.weight * 1000) / 1000 },
          })),
        },
      ],
      filters: {
        type_filters: { filters: { rarity: { option: 'nonunique' } } },
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
