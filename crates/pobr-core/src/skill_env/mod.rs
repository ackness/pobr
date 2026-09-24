//! Skill environment — pure data access for skill-group → modifier translation,
//! decoupled from `Build`/`BuildData`.
//!
//! The orchestrator (pobr-build) assembles a [`SkillEnv`] from `BuildData` and passes
//! it to the engine-semantics functions. This lets the same functions be reused by
//! WASM, CLI, and test harnesses without going through the full orchestrator.

mod item_text;
mod mods;
mod support;

pub use item_text::{
    clean_item_text, parse_adds_physical, parse_adds_with_suffix, parse_local_defence_flat,
    parse_local_defence_inc, parse_weapon_local_crit, weapon_mod_texts,
};
pub use mods::{
    dot_flag_modifiers, is_off_hand_weapon_base_stat, resolved_enemy_level, skill_base_modifiers,
    support_mana_multiplier_modifier,
};
pub use support::{
    CompatibleSupport, GroupSupportJudgement, SupportCandidate, judge_group_supports,
};
