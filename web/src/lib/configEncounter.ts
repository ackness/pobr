import type { CalculateBuildResponse, ConfigInputValue, ConfigOption, EnemyTier, SocketGroupInput } from '../api/types';
import type { EnemyPreset, EnemyPresets } from '../api/enemyPresets';
import { resolveEnemyTier } from '../app/enemy';
import { configDefaultValue } from './configDefaults';
import { bindT, enemyTierLabel, type Lang } from './i18n';
import { formatNumber } from './statDisplay';

const ENEMY_PRESET_PARAMETERS = new Set([
  'enemyFireResist', 'enemyColdResist', 'enemyLightningResist', 'enemyChaosResist',
  'enemyArmour', 'enemyEvasion', 'enemyPhysicalReduction', 'enemyLevel',
]);

export function inheritsEnemyValue(option: Pick<ConfigOption, 'var'>): boolean {
  return ENEMY_PRESET_PARAMETERS.has(option.var);
}

/** Only the actor behind the headline can supply damage-type shortcuts. */
export function encounterHitTypes(calc: CalculateBuildResponse | null | undefined): Set<string> {
  // Production minion output has no damage-type rows, including when details are requested.
  if (calc?.main_minion_index != null) return new Set();
  const parts = calc?.main_skill?.hit_damage ?? [];
  return new Set(parts.filter(part => Number.isFinite(part.avg) && part.avg > 0).map(part => part.damage_type));
}

// These effects gain energy per enemy power in the pinned granted-effect stats
// consumed by skill_env/triggers.rs. Unknown/new effects are deliberately omitted.
const ENEMY_POWER_META_SKILLS = new Set([
  'MetaCastOnCritPlayer', 'MetaCastOnFreezePlayer', 'MetaCastOnIgnitePlayer', 'MetaCastOnShockPlayer',
]);

/** A confirmed trigger and its active meta gem must both identify enemy-power-dependent generation. */
export function hasEnergyTrigger(calc: CalculateBuildResponse | null | undefined, groups: readonly SocketGroupInput[] = []): boolean {
  const trigger = calc?.energy_trigger;
  const main = calc?.main_skill;
  if (!trigger || !(trigger.maximum_energy > 0) || !trigger.source || trigger.at_own_rate || trigger.missing_source
    || calc?.main_minion_index != null || !main || ['Block', 'MinionDeath'].includes(trigger.source)) return false;
  const group = groups[main.group_index];
  if (!group?.enabled) return false;
  if (!groups.some(candidate => candidate.enabled && candidate.gems.some(gem => gem.enabled !== false && gem.skill_id === trigger.source))) return false;
  const metas = group.gems.filter(gem => gem.enabled !== false && gem.skill_id.startsWith('Meta'));
  return metas.length === 1 && ENEMY_POWER_META_SKILLS.has(metas[0].skill_id);
}

export function encounterPreset(table: EnemyPresets | null, tier: EnemyTier | 'custom'): EnemyPreset | undefined {
  return tier === 'custom' ? undefined : table?.tiers.find(preset => preset.id.toLowerCase() === tier);
}

/** Decode a ratio written directly in the preset; this is not monster level scaling. */
export function presetRatio(value: EnemyPreset['armour_mult_pct']): number {
  return value.base + value.num / value.den;
}

/** Base inputs only: final combat resistance includes curses/exposure and is not an inherited field value. */
export function encounterInheritedValues(table: EnemyPresets | null, tier: EnemyTier | 'custom', characterLevel: number | undefined,
  inputs: Record<string, ConfigInputValue>): Record<string, number> {
  const preset = encounterPreset(table, tier);
  if (!preset || !table) return {};
  const values: Record<string, number> = {
    enemyFireResist: preset.elemental_resist_bonus,
    enemyColdResist: preset.elemental_resist_bonus,
    enemyLightningResist: preset.elemental_resist_bonus,
    enemyChaosResist: preset.chaos_resist_bonus,
  };
  const explicit = inputs.enemyLevel;
  const level = typeof explicit === 'number' && Number.isFinite(explicit) && explicit > 0 ? explicit : characterLevel;
  if (level !== undefined && Number.isFinite(level)) values.enemyLevel = Math.max(1, Math.min(table.max_enemy_level, Math.max(preset.min_level, Math.floor(level))));
  // Armour/evasion ratings need engine-owned monster scaling, so are intentionally absent.
  return values;
}

export function encounterTierLabel(lang: Lang, tier: Parameters<typeof resolveEnemyTier>[0], inputs: Record<string, unknown> | undefined): string {
  const resolved = resolveEnemyTier(tier, inputs);
  return resolved === 'custom' ? bindT(lang)('config.customEnemy') : enemyTierLabel(lang, resolved);
}

/** Prefer explicit inputs, then pinned preset facts; catalog examples are not enemy defaults. */
export function encounterValue(option: ConfigOption, raw: ConfigInputValue | undefined, inherited: Readonly<Record<string, number>> = {}): ConfigInputValue | undefined {
  return raw ?? (inheritsEnemyValue(option) ? inherited[option.var] : configDefaultValue(option));
}

export function encounterValueLabel(lang: Lang, option: ConfigOption, value: ConfigInputValue | undefined): string {
  const tt = bindT(lang);
  if (value === undefined) return tt('config.valueUnavailable');
  if (option.input_type === 'check' && (typeof value === 'boolean' || typeof value === 'number'))
    return tt(value ? 'config.enabled' : 'config.disabled');
  if (typeof value === 'number' && Number.isFinite(value)) {
    const unit = /Resist$|Reduction$/.test(option.var) || option.var === 'conditionShockEffect' ? '%' : '';
    return `${formatNumber(value, 2, 0)}${unit}`;
  }
  // A saved string must survive round trips, but is not evidence of a numeric override.
  return tt('config.checkSavedValue');
}
