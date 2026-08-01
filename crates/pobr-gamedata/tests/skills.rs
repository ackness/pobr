//! Data loading: skill gems / overrides / buffs / triggers / minions /
//! monster scaling / enemy presets / ailments.
//!
//! An aggregated binary: formerly-separate test files merged into
//! submodules (26→4), reducing the number of linked binaries to speed up builds.
#![allow(clippy::all)]

#[path = "skills/load_buff_definitions.rs"]
mod load_buff_definitions;
#[path = "skills/load_enemy_presets.rs"]
mod load_enemy_presets;
#[path = "skills/load_minions.rs"]
mod load_minions;
#[path = "skills/load_monster_scaling.rs"]
mod load_monster_scaling;
#[path = "skills/load_non_damaging_ailments.rs"]
mod load_non_damaging_ailments;
#[path = "skills/load_skill_gems.rs"]
mod load_skill_gems;
#[path = "skills/load_skill_overrides.rs"]
mod load_skill_overrides;
#[path = "skills/load_trigger_configs.rs"]
mod load_trigger_configs;
#[path = "skills/skill_overrides_migration.rs"]
mod skill_overrides_migration;
