//! Skill / DPS / mechanics end-to-end tests (damage, dual wield, cost, minions, reservation, quality, gating, corpse explosion).
//!
//! Aggregated test binary: originally separate test files, merged into submodules (22→4) to cut the number of linked test binaries and speed up builds.
#![allow(clippy::all)]

#[path = "skills/aura_gems.rs"]
mod aura_gems;
#[path = "skills/buff_stack_limits.rs"]
mod buff_stack_limits;
#[path = "skills/charge_consuming_buffs.rs"]
mod charge_consuming_buffs;
#[path = "skills/charge_regulation.rs"]
mod charge_regulation;
#[path = "skills/corpse_explosion.rs"]
mod corpse_explosion;
#[path = "skills/corrupted_gems.rs"]
mod corrupted_gems;
#[path = "skills/cost_conversion.rs"]
mod cost_conversion;
#[path = "skills/cost_multiplier.rs"]
mod cost_multiplier;
#[path = "skills/dual_wield.rs"]
mod dual_wield;
#[path = "skills/equipment_affix_effects.rs"]
mod equipment_affix_effects;
#[path = "skills/gem_quality.rs"]
mod gem_quality;
#[path = "skills/minions.rs"]
mod minions;
#[path = "skills/skill_damage_dps.rs"]
mod skill_damage_dps;
#[path = "skills/skill_metrics.rs"]
mod skill_metrics;
#[path = "skills/spirit_reservation.rs"]
mod spirit_reservation;
#[path = "skills/support_gating.rs"]
mod support_gating;
#[path = "skills/support_gem_count.rs"]
mod support_gem_count;
#[path = "skills/support_granted_skills.rs"]
mod support_granted_skills;
#[path = "skills/weapon_accuracy_damage.rs"]
mod weapon_accuracy_damage;
#[path = "skills/withered.rs"]
mod withered;

#[path = "skills/passive_jewels.rs"]
mod passive_jewels;
