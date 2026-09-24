//! Skill environment — pure data access for skill-group → modifier translation,
//! decoupled from `Build`/`BuildData`.
//!
//! The orchestrator (pobr-build) assembles a [`SkillEnv`] from `BuildData` and passes
//! it to the engine-semantics functions. This lets the same functions be reused by
//! WASM, CLI, and test harnesses without going through the full orchestrator.

mod mods;
mod support;

pub use mods::{dot_flag_modifiers, is_off_hand_weapon_base_stat, resolved_enemy_level};
pub use support::{
    CompatibleSupport, GroupSupportJudgement, SupportCandidate, judge_group_supports,
};
