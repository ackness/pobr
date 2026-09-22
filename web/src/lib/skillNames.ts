import type { GemCatalogEntry } from '../api/types';
import type { Lang } from './i18n';

export function prettySkillId(id: string): string {
  return id.replace(/Player(Two)?$/, '').replace(/^Support/, '').replace(/([a-z0-9])([A-Z])/g, '$1 $2');
}

export function gemDisplayName(entry: GemCatalogEntry, lang: Lang): string {
  if (lang === 'zh-CN') return entry.name_zh_cn ?? entry.name_zh_tw ?? entry.name;
  if (lang === 'zh-TW') return entry.name_zh_tw ?? entry.name_zh_cn ?? entry.name;
  return entry.name;
}

/** Resolve secondary skills through the data's gem links, without inventing picker entries. */
export function skillDisplayName(id: string, lang: Lang, byId: ReadonlyMap<string, GemCatalogEntry>): string {
  const primary = byId.get(id);
  if (primary) return gemDisplayName(primary, lang);
  if (lang !== 'en-US') {
    for (const gem of byId.values()) {
      const index = gem.additional_skill_ids?.indexOf(id) ?? -1;
      if (index < 0) continue;
      const kind = id.startsWith('Triggered') ? (lang === 'zh-CN' ? '触发' : '觸發') : '附属技能';
      const suffix = gem.additional_skill_ids!.length > 1 ? ` ${index + 1}` : '';
      return `${gemDisplayName(gem, lang)}（${lang === 'zh-TW' ? kind.replace('属', '屬') : kind}${suffix}）`;
    }
  }
  return prettySkillId(id);
}
