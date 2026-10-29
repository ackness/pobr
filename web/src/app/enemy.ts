import type { EnemyTier } from '../api/types';

/** The engine's `EnemyTier::default()`: a request without `enemy_tier` is calculated against a Pinnacle boss. */
export const ENGINE_DEFAULT_TIER: EnemyTier = 'pinnacle';

/**
 * The enemy the numbers are measured against. An unset tier is the engine default, unless the BD
 * carries its own `enemyIsBoss` input that is not a known tier: then the engine reads that input and
 * the honest answer is "custom".
 */
export function resolveEnemyTier(tier: EnemyTier | undefined, configInputs: Record<string, unknown> | undefined): EnemyTier | 'custom' {
  if (tier) return tier;
  return configInputs?.enemyIsBoss !== undefined ? 'custom' : ENGINE_DEFAULT_TIER;
}
