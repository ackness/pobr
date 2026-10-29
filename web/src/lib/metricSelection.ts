import type { DisplayStatValue } from '../api/types';

/** A small, stable starting point. All fields remain searchable, including zero values. */
export const COMMON_METRICS = [
  'CombinedDPS', 'TotalDPS', 'AverageDamage', 'Speed', 'HitChance', 'CritChance',
  'Life', 'EnergyShield', 'Mana', 'SpiritUnreserved', 'TotalEHP',
  'PhysicalMaxHit', 'FireResist', 'ColdResist', 'LightningResist', 'ChaosResist',
  'Armour', 'Evasion', 'LifeRegen', 'ManaRegen',
] as const;

export function defaultMetric(stats: readonly Pick<DisplayStatValue, 'id' | 'value'>[], hasSkill: boolean): string | null {
  const available = new Set(stats.filter(stat => typeof stat.value === 'number' && Number.isFinite(stat.value)).map(stat => stat.id));
  const preferred = hasSkill ? ['CombinedDPS', 'TotalDPS', 'Life', 'EnergyShield'] : ['Life', 'EnergyShield', 'Mana'];
  return preferred.find(id => available.has(id)) ?? stats.find(stat => available.has(stat.id))?.id ?? null;
}
