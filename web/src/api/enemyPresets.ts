/** Display subset of the pinned base/enemy_presets.json snapshot. */
export interface EnemyPreset {
  id: string;
  min_level: number;
  elemental_resist_bonus: number;
  chaos_resist_bonus: number;
  armour_mult_pct: { base: number; num: number; den: number };
  evasion_mult_pct: { base: number; num: number; den: number };
  enemy_mods: { name: string; mod_type: string; value: number; effective_only: boolean }[];
}

export interface EnemyPresets {
  max_enemy_level: number;
  tiers: EnemyPreset[];
}
