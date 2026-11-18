import type { EnemyTier } from '../api/types';
import { t, enemyTierLabel, type Lang } from '../lib/i18n';
import { colon } from './text';

const TIERS: EnemyTier[] = ['none', 'boss', 'pinnacle', 'uber'];

/** The enemy choices that match what was typed, matched on their own labels only so "boss" never lists the normal enemy first. */
export function matchingTiers(query: string, lang: Lang): { tier: EnemyTier; label: string }[] {
  const q = query.trim().toLowerCase();
  const prefix = t(lang, 'compare.enemy');
  return TIERS.map(tier => ({ tier, label: `${prefix}${colon(lang)}${enemyTierLabel(lang, tier)}` }))
    .filter(entry => !q || entry.label.toLowerCase().includes(q));
}
