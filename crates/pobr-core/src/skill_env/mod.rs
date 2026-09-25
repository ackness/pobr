//! Skill environment — pure data access for skill-group → modifier translation,
//! decoupled from `Build`/`BuildData`.
//!
//! The orchestrator (pobr-build) assembles a [`SkillEnv`] from `BuildData` and passes
//! it to the engine-semantics functions. This lets the same functions be reused by
//! WASM, CLI, and test harnesses without going through the full orchestrator.

pub mod buff_stat_map;
mod buffs;
mod item_text;
mod minions;
mod lookup;
mod mods;
mod resolve;
mod support;
mod weapon;

pub use item_text::{
    clean_item_text, is_local_spirit_mod, is_weapon_local_mod, item_local_defence_flat,
    item_local_defence_inc, item_per_level_defence, parse_adds_physical, parse_adds_with_suffix,
    parse_has_per_level_defence, parse_local_defence_flat, parse_local_defence_inc,
    parse_weapon_local_crit, weapon_local_attack_speed, weapon_local_phys_adds,
    weapon_local_phys_inc, weapon_mod_texts,
};
pub use lookup::{
    ArmourBaseLookup, BaseItemLookup, CostTypeLookup, CostTypeRef, EffectLookup,
    EffectStats, EnabledGroup, EquipmentView, GemDefLookup, MinionLookup,
    ParserRulesLookup, PassiveNodeLookup, SelectedSetLevelRow, SocketGroupView,
    StatMapLookup, StatSetLookup, TriggerConfigLookup, UnarmedDataLookup,
    UnselectedSetStats, WeaponBaseLookup, WeaponTypeLookup,
};
pub use resolve::{
    GemPropertyLookup, GemPropertyScanView, GrantedPassiveLookup, ResolvedCost,
    ResolvedSkillLevel, SkillLevelLookup, additional_gem_levels, gem_property_applies,
    gem_property_bonuses, gemling_quality_flag, granted_passive_stats,
    resolve_skill_level, support_granted_gem_levels, support_stat_set_index,
};
pub use mods::{
    GemInput, GemPropertyBonus, GemPropertyKind, adorned_corrupted_magic_jewel_inc,
    clean_grant_text, crossbow_reload_modifiers, data_mapped_stat_modifiers, dot_flag_modifiers,
    gem_level_category_matches, is_attribute_node, is_damage_skill, is_off_hand_weapon_base_stat,
    mk_trigger_flag, mk_trigger_mod, parse_gem_level_bonus, parse_gem_property_bonus,
    pick_group_main_skill, resolved_enemy_level, scale_trunc_2dp, skill_base_modifiers,
    skill_name_from_id, skill_type_bits, skill_type_flags, support_mana_multiplier_modifier,
    vendor_scale_mod_value,
};
pub use support::{
    CompatibleSupport, GroupSupportJudgement, SupportCandidate, judge_group_supports,
};
pub use weapon::{
    WeaponContribution, WeaponContributionLookup, WeaponItemLookup,
    dual_wield_off_hand_contribution, non_weapon_attack_contribution, off_hand_defence,
    per_shield_defence_scale, unarmed_contribution, weapon_contribution,
    weapon_item_contribution,
};
pub use buffs::{
    ReservationDb, ReservationLookup, buff_skill_name, herald_skill_names,
    self_buff_offensive_modifiers, spirit_reservation_modifiers,
};
pub use minions::{MinionEnv, MinionSession, spawn_minions};
