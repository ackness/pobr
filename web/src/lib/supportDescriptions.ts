import descriptions from './supportDescriptions.zh-CN.json';
import { chineseScript } from './chinese';
import type { Lang } from './i18n';
import type { TradeGem } from './tradeOptimizer';

const entries: Readonly<Record<string, { en: string; zh_cn: string }>> = descriptions.entries;

/** A stable ID alone is insufficient: changed source prose must fall back rather than show a stale translation. */
export function supportDescription(gem: Pick<TradeGem, 'skill_id' | 'description'>, lang: Lang) {
  const english = gem.description?.trim();
  if (!english) return { text: '', lang, source: 'unavailable' as const };
  const entry = entries[gem.skill_id];
  if (lang !== 'en-US' && entry?.en === english) {
    return { text: lang === 'zh-CN' ? entry.zh_cn : chineseScript(entry.zh_cn, lang), lang, source: 'dictionary' as const };
  }
  return { text: gem.description!, lang: 'en' as const, source: 'english' as const };
}
