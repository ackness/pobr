//! Data lookup traits — the `BuildData` query surface abstracted for engine-semantics
//! functions. `BuildData` implements these; the orchestrator passes `&dyn Trait` into
//! the pure functions.

use pobr_data::catalog::DotFlags;
use pobr_data::catalog::{
    ArmourBaseStats, BaseItemDef, GrantedEffectDef, PassiveNodeDef, SkillDamageStat,
    SkillGemDef, SkillLevelDef, TriggerConfigDef, UnarmedWeaponDef, WeaponBaseStats,
    WeaponTypeDef,
};
use pobr_data::minion::MinionDef;

use crate::skill_env::GemInput;

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

    /// Returns the alt-quality-stacking stats for `effect_id` at `quality` (alt rows
    /// only; consumed under the GemlingQuality flag, matching vendor's
    /// `useAltGemQualityStats`).
    fn alt_quality_stats(&self, effect_id: &str, quality: u32) -> Vec<SkillDamageStat> {
        let _ = (effect_id, quality);
        Vec::new()
    }

    /// Returns the unselected statSets' merged stat snapshots for `effect_id`
    /// (the data source for the global-only merge; empty when there's no stat-set data).
    fn unselected_set_stats(
        &self,
        effect_id: &str,
        level: u32,
        quality: u32,
        set_index: Option<u32>,
    ) -> Vec<UnselectedSetStats> {
        let _ = (effect_id, level, quality, set_index);
        Vec::new()
    }

    /// Returns the selected statSet's per-level row for `effect_id` at `level`
    /// (the selected set's `damage_multiplier` source; `None` = no stat-set data).
    fn selected_set_level_row(
        &self,
        effect_id: &str,
        level: u32,
        set_index: Option<u32>,
    ) -> Option<SelectedSetLevelRow> {
        let _ = (effect_id, level, set_index);
        None
    }
}

/// The selected statSet's per-level row projection (the fields `resolve_skill_level`
/// needs beyond `effect_stats`: damage multiplier + inherent attack-speed MORE).
#[derive(Debug, Clone, Copy, Default)]
pub struct SelectedSetLevelRow {
    /// Skill damage multiplier (PoB `baseMultiplier`; `1.0` = none).
    pub damage_multiplier: f64,
    /// statSet `baseMods`' inherent attack speed MORE (percentage points; `None` = none).
    pub skill_attack_speed_more: Option<f64>,
}

/// A resolved stat snapshot for one granted effect at a specific level/quality/set.
#[derive(Debug, Clone, Default)]
pub struct EffectStats {
    /// The statSet's base damage stats.
    pub base: Vec<SkillDamageStat>,
    /// The quality-stacking segment.
    pub quality: Vec<SkillDamageStat>,
}

impl EffectStats {
    /// A merged view chaining base + quality in order (for consumers that don't need attribution).
    pub fn all(&self) -> impl Iterator<Item = &SkillDamageStat> {
        self.base.iter().chain(self.quality.iter())
    }
}

/// A stat snapshot for one **unselected statSet** of a granted effect (the data source
/// for the global-only merge, see [`StatSetLookup::unselected_set_stats`]).
#[derive(Debug, Clone, PartialEq)]
pub struct UnselectedSetStats {
    /// statmap per-set override lookup key = the decimal string of vendor's 1-based
    /// export index (following the key convention of
    /// [`pobr_data::catalog::stat_map::SkillStatMapDef::per_stat_set`], fed directly as
    /// `stat_map_engine::map_stat_global_only`'s `set_key`).
    pub set_key: String,
    /// statSet's stable id (for attribution labels / debugging, e.g.
    /// `FlameWallProjectileBuffPlayer`).
    pub set_id: String,
    /// This set's stat table at (gem_level, quality) — same stats already merged
    /// additively, sorted by stat name (matching vendor's `buildSkillInstanceStats`
    /// table semantics, CalcTools.lua:138-200).
    pub stats: Vec<SkillDamageStat>,
}

/// Equipment view (the `Build.items` domain) — read-only access to equipped items.
pub trait EquipmentView {
    /// Returns the item in `slot`, or `None` if empty.
    fn item(&self, slot: pobr_data::item::EquipmentSlot) -> Option<&pobr_data::item::Item>;
}

/// Socket-group view (the `Build.socket_groups` domain) — read-only access to the
/// enabled gem groups' gem lists.
pub trait SocketGroupView {
    /// Calls `f` for every enabled socket group, in build order. The callback receives
    /// the group's gem list and the group's `from_gem` flag (whether the group is a
    /// real gem group vs an item/other source — vendor's `gemSource`/`fromItem` split).
    /// Returning an iterator of `&SocketGroup` would tie the trait to the build's
    /// concrete group type, so the callback shape is used instead.
    fn for_each_enabled_group(&self, f: &mut dyn FnMut(EnabledGroup<'_>));
}

/// One enabled socket group, as seen by engine-semantics functions.
#[derive(Debug, Clone, Copy)]
pub struct EnabledGroup<'a> {
    /// The group's gems (granted effect ids + level/quality/set selection).
    pub gems: &'a [GemInput],
    /// Whether the group comes from a real gem (vs an item-granted / other source:
    /// vendor's `source="Item:…"` groups don't pass the support-gems-only gate).
    pub from_gem: bool,
    /// The group's socket slot label (e.g. `"weapon1"`; `None` = slotless source).
    pub slot: Option<&'a str>,
    /// The builder-path fallback active skill (PoB `<Gem skillId>`) — only consulted
    /// when `gems` is empty (a `with_active_skill`-constructed group).
    pub active_skill_id: Option<&'a str>,
    /// The fallback active gem level paired with `active_skill_id`.
    pub active_gem_level: u32,
}

/// Weapon base stats lookup (the `base_items` + `weapon_types` domains).
pub trait WeaponBaseLookup {
    /// Returns the weapon base stats for `base_name`, or `None` if not a weapon base.
    fn weapon_base(&self, base_name: &str) -> Option<&WeaponBaseStats>;
}

/// Armour base stats lookup (the `base_items` domain's armour segment).
pub trait ArmourBaseLookup {
    /// Returns the armour base stats for `base_name`, or `None` if not armour.
    fn armour_base(&self, base_name: &str) -> Option<&ArmourBaseStats>;
}

/// Base item definition lookup (the `base_items` domain).
pub trait BaseItemLookup {
    /// Returns the base item definition for `base_name`.
    fn base_item(&self, base_name: &str) -> Option<&BaseItemDef>;
}

/// Weapon type table lookup (`base/weapon_types.json`, the `data.weaponTypeInfo`
/// counterpart). The implementor owns the GGG `item_class` → table-key mapping
/// (e.g. `Warstaff` → `Staff`, plain `Staff` → unarmed `None`).
pub trait WeaponTypeLookup {
    /// Returns the weapon type info for a GGG `item_class` name.
    fn weapon_type_info(&self, item_class: &str) -> Option<&WeaponTypeDef>;
}

/// Per-class unarmed base table lookup (`base/unarmed_data.json`).
pub trait UnarmedDataLookup {
    /// Returns the unarmed base entry for an English class name.
    fn unarmed_for_class(&self, class_name: &str) -> Option<&UnarmedWeaponDef>;
}

/// Cost-type table lookup (the `CostTypes` domain: resource id + divisor + per-minute).
pub trait CostTypeLookup {
    /// Returns the cost type at `index` (`None` = out of range / no table).
    fn cost_type(&self, index: usize) -> Option<CostTypeRef<'_>>;
}

/// A resolved `CostTypes` row (resource id + divisor + per-minute flag).
#[derive(Debug, Clone, Copy)]
pub struct CostTypeRef<'a> {
    /// Resource id (`Mana` / `Life` / `ES` / `ManaPerMinute` …).
    pub id: &'a str,
    /// Value divisor (1 for instantaneous costs; 60 for per-minute resources).
    pub divisor: u32,
    /// Whether this is a resource consumed continuously over time.
    pub per_minute: bool,
}

/// Passive tree node lookup (the `passive_nodes` domain).
pub trait PassiveNodeLookup {
    /// Returns the passive node definition for `node_id`.
    fn passive_node(&self, node_id: u32) -> Option<&PassiveNodeDef>;

    /// Returns the Notable node whose name matches `name` case-insensitively
    /// (the `GrantedPassive` enchant resolution path; `None` = unknown name).
    fn notable_by_name(&self, name: &str) -> Option<&PassiveNodeDef>;
}

/// Gem base definition lookup (`gem_effects` FK → `skill_gems` domain), for the
/// `gemRequirements` attribute-weight check (vendor `effect.gemData[reqX] > 0`).
pub trait GemDefLookup {
    /// Returns the skill gem definition hosting `effect_id` (resolved via the
    /// `gem_effects` foreign key), or `None` when unresolvable.
    fn gem_def_for_effect(&self, effect_id: &str) -> Option<&SkillGemDef>;
}

/// Parser-rules lookup (the `parser_rules` domain — the compiled mod-parser engine).
pub trait ParserRulesLookup {
    /// Returns the compiled parser rules, or `None` when the domain is missing
    /// (an old data pack — every line parses to whole-line Unsupported).
    fn parser_rules(&self) -> Option<&crate::mod_parser::CompiledParserRules>;
}

/// Stat-map catalog lookup (the `stat_map_catalog` domain).
pub trait StatMapLookup {
    /// Returns the stat-map catalog, or `None` when the overlay is missing.
    fn stat_map_catalog(&self) -> Option<&crate::rules::stat_map_engine::StatMapCatalog>;
}

/// Minion catalog lookup (the `minions` domain + the effect's `minion_list` FK).
pub trait MinionLookup {
    /// Returns the minion definition for `id`.
    fn minion_def(&self, id: &str) -> Option<&MinionDef>;

    /// Returns the list of minions `effect_id` summons (empty for a non-summon skill).
    fn effect_minion_list(&self, effect_id: &str) -> &[String];
}

/// Trigger-config table lookup (the `trigger_configs` domain, keyed by effect id).
pub trait TriggerConfigLookup {
    /// Returns the trigger config entry matching `effect_id`.
    fn trigger_config(&self, effect_id: &str) -> Option<&TriggerConfigDef>;
}
