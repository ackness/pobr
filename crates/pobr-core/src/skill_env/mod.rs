//! Skill environment — pure data access for skill-group → modifier translation,
//! decoupled from `Build`/`BuildData`.
//!
//! The orchestrator (pobr-build) assembles a [`SkillEnv`] from `BuildData` and passes
//! it to the engine-semantics functions. This lets the same functions be reused by
//! WASM, CLI, and test harnesses without going through the full orchestrator.

mod buff_specs;
pub mod buff_stat_map;
mod buffs;
mod item_text;
mod lookup;
mod minions;
mod mods;
mod resolve;
mod stat_map;
mod support;
mod triggers;
mod weapon;

pub use buff_specs::{
    BuffEnv, buff_skill_specs, exposure_support_modifiers, group_judgement, support_buff_specs,
    support_modifiers, warcry_skill_specs,
};
pub use buffs::{
    ReservationDb, ReservationLookup, buff_skill_name, herald_skill_names,
    self_buff_offensive_modifiers, spirit_reservation_modifiers,
};
pub use item_text::{
    clean_item_text, is_local_spirit_mod, is_weapon_local_mod, item_local_defence_flat,
    item_local_defence_inc, item_per_level_defence, parse_adds_physical, parse_adds_with_suffix,
    parse_has_per_level_defence, parse_local_defence_flat, parse_local_defence_inc,
    parse_weapon_local_crit, weapon_local_attack_speed, weapon_local_phys_adds,
    weapon_local_phys_inc, weapon_mod_texts,
};
pub use lookup::{
    ArmourBaseLookup, BaseItemLookup, CostTypeLookup, CostTypeRef, EffectLookup, EffectStats,
    EnabledGroup, EquipmentView, GemDefLookup, MinionLookup, ParserRulesLookup, PassiveNodeLookup,
    SelectedSetLevelRow, SocketGroupView, StatMapLookup, StatSetLookup, TriggerConfigLookup,
    UnarmedDataLookup, UnselectedSetStats, WeaponBaseLookup, WeaponTypeLookup,
};
pub use minions::{MinionEnv, MinionSession, spawn_minions};
pub use mods::{
    GemInput, GemPropertyBonus, GemPropertyKind, adorned_corrupted_magic_jewel_inc,
    clean_grant_text, crossbow_reload_modifiers, data_mapped_stat_modifiers, dot_flag_modifiers,
    gem_level_category_matches, is_attribute_node, is_damage_skill, is_off_hand_weapon_base_stat,
    mk_trigger_flag, mk_trigger_mod, parse_gem_level_bonus, parse_gem_property_bonus,
    pick_group_main_skill, resolved_enemy_level, scale_trunc_2dp, skill_base_modifiers,
    skill_name_from_id, skill_type_bits, skill_type_flags, support_mana_multiplier_modifier,
    vendor_scale_mod_value,
};
pub use resolve::{
    GemPropertyLookup, GemPropertyScanView, GrantedPassiveLookup, ResolvedCost, ResolvedSkillLevel,
    SkillLevelLookup, additional_gem_levels, gem_property_applies, gem_property_bonuses,
    gemling_quality_flag, granted_passive_stats, resolve_skill_level, support_granted_gem_levels,
    support_stat_set_index,
};
pub use stat_map::{
    StatMapCompareRecord, StatMapCtx, StatMapMode, curse_stat_modifiers,
    data_mapped_stat_modifiers as stat_map_data_mapped, debuff_stat_modifiers, has_debuff_payload,
    has_exposure_inflict_stats, mapped_stat_modifiers, player_buff_stat_modifiers,
};
pub use support::{
    CompatibleSupport, GroupSupportJudgement, SupportCandidate, judge_group_supports,
};
pub use triggers::{
    RecognizedTrigger, TriggerCtx, TriggerEnv, TriggerSubCalc, push_source_stat_mods,
    recognize_trigger_config, source_cond_matches, trigger_modifiers,
};
pub use weapon::{
    WeaponContribution, WeaponContributionLookup, WeaponItemLookup,
    dual_wield_off_hand_contribution, non_weapon_attack_contribution, off_hand_defence,
    per_shield_defence_scale, unarmed_contribution, weapon_contribution, weapon_item_contribution,
};
