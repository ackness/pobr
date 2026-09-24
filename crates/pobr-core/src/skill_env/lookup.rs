//! Data lookup traits — the `BuildData` query surface abstracted for engine-semantics
//! functions. `BuildData` implements these; the orchestrator passes `&dyn Trait` into
//! the pure functions.

use pobr_data::catalog::DotFlags;
use pobr_data::catalog::{GrantedEffectDef, SkillDamageStat, SkillLevelDef};

/// Granted-effect lookup (the `granted_effects` + `gem_effects` domains).
pub trait EffectLookup {
    /// Returns the granted effect definition for `id`, or `None` if unknown.
    fn effect(&self, id: &str) -> Option<&GrantedEffectDef>;

    /// Returns the additional granted effect ids linked to `primary_id` via the
    /// `gem_effects` foreign key (empty when the domain is missing).
    fn additional_effects(&self, primary_id: &str) -> &[String];
}

/// Per-level stat table lookup (the `granted_effect_levels` + `skill_stat_sets` +
/// `gem_quality_stats` domains).
pub trait StatSetLookup {
    /// Returns the resolved damage stats for `effect_id` at `(level, quality, set_index)`.
    fn effect_stats(
        &self,
        effect_id: &str,
        level: u32,
        quality: u32,
        set_index: Option<u32>,
    ) -> EffectStats;

    /// Returns the selected statSet's override key for `effect_id`.
    fn selected_set_key(&self, effect_id: &str, set_index: Option<u32>) -> Option<String>;

    /// Returns the selected statSet's dot flags for `effect_id`.
    fn selected_set_dot_flags(&self, effect_id: &str, set_index: Option<u32>) -> DotFlags;

    /// Returns the selected statSet's corpse-explosion gate for `effect_id`.
    fn selected_set_explode_corpse(&self, effect_id: &str, set_index: Option<u32>) -> bool;

    /// Returns the per-level parameter row for `effect_id` at `level` (last row ≤ level).
    fn effect_level_row(&self, effect_id: &str, level: u32) -> Option<&SkillLevelDef>;

    /// Returns the quality-stacking stats for `effect_id` at `quality` (non-alt rows only).
    fn quality_stats(&self, effect_id: &str, quality: u32) -> Vec<SkillDamageStat>;
}

/// A resolved stat snapshot for one granted effect at a specific level/quality/set.
#[derive(Debug, Clone, Default)]
pub struct EffectStats {
    /// The statSet's base damage stats.
    pub base: Vec<SkillDamageStat>,
    /// The quality-stacking segment.
    pub quality: Vec<SkillDamageStat>,
}

/// Equipment view (the `Build.items` domain) — read-only access to equipped items.
pub trait EquipmentView {
    /// Returns the item in `slot`, or `None` if empty.
    fn item(&self, slot: pobr_data::item::EquipmentSlot) -> Option<&pobr_data::item::Item>;
}

/// Weapon base stats lookup (the `base_items` + `weapon_types` domains).
pub trait WeaponBaseLookup {
    /// Returns the weapon base stats for `base_name`, or `None` if not a weapon base.
    fn weapon_base(&self, base_name: &str) -> Option<&pobr_data::catalog::WeaponBaseStats>;
}

/// Stat-map catalog lookup (the `stat_map_catalog` domain).
pub trait StatMapLookup {
    /// Returns the stat-map catalog, or `None` when the overlay is missing.
    fn stat_map_catalog(&self) -> Option<&crate::rules::stat_map_engine::StatMapCatalog>;
}
