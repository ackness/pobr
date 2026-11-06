import { localizedName } from './chinese';
import type { GemCatalogEntry } from '../api/types';
import { t, type Lang } from './i18n';

/** Groups the build decoder synthesizes from PoB (not gems), so the gem catalog never has them. */
const SYNTHETIC_SKILL_LABELS = {
  ThornsPlayer: 'skill.synthetic.ThornsPlayer',
  EnemyExplode: 'skill.synthetic.EnemyExplode',
} as const;

export function prettySkillId(id: string): string {
  return id.replace(/Player(Two)?$/, '').replace(/^Support/, '').replace(/([a-z0-9])([A-Z])/g, '$1 $2');
}

export function gemDisplayName(entry: GemCatalogEntry, lang: Lang): string {
  const name = localizedName(lang, entry.name, entry.name_zh_cn, entry.name_zh_tw);
  // Companion and Spectre names reserve a suffix for the chosen monster.
  // Generic gem labels omit it; the minion controls show the monster's name.
  const cleaned = normalizeGemName(name.replace(/[:：]\s*\{0\}$/, '').trim());
  return markUntranslated(cleaned, normalizeGemName(entry.name.replace(/[:：]\s*\{0\}$/, '').trim()), lang);
}

/** One space before a gem tier, including names imported without the space. */
export function normalizeGemName(name: string): string {
  return name.replace(/\s*([IVX]+)$/, (match, tier: string, offset: number) => offset > 0 ? ` ${tier}` : match);
}

export function markUntranslated(name: string, original: string, lang: Lang): string {
  return lang !== 'en-US' && name === original && /[A-Za-z]/.test(original)
    ? t(lang, 'skills.untranslated').replace('{name}', name) : name;
}

/** Forms share the gem/effect resolver before falling back to the original label. */
export function skillFormName(label: string, id: string, lang: Lang, catalog: ReadonlyMap<string, GemCatalogEntry>): string {
  if (lang === 'en-US') return normalizeGemName(label);
  const primary = catalog.get(id);
  if (primary && (label === primary.name || label === prettySkillId(id))) return gemDisplayName(primary, lang);
  const named = [...new Set([...catalog.values()].filter(gem => label === gem.name).map(gem => gemDisplayName(gem, lang)))];
  if (named.length === 1) return named[0];
  const forms = { Projectile: 'skills.formProjectile', Explosion: 'skills.formExplosion' } as const;
  if (Object.hasOwn(forms, label)) return t(lang, forms[label as keyof typeof forms]);
  return markUntranslated(normalizeGemName(label), normalizeGemName(label), lang);
}

/** Resolve secondary skills through the data's gem links, without inventing picker entries. */
export function skillDisplayName(id: string, lang: Lang, byId: ReadonlyMap<string, GemCatalogEntry>, originalName?: string): string {
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
  if (Object.hasOwn(SYNTHETIC_SKILL_LABELS, id)) return t(lang, SYNTHETIC_SKILL_LABELS[id as keyof typeof SYNTHETIC_SKILL_LABELS]);
  const fallback = normalizeGemName(originalName || prettySkillId(id));
  return markUntranslated(fallback, fallback, lang);
}
