//! Skill / DPS / mechanics end-to-end tests (damage, dual wield, cost, minions, reservation, quality, gating, corpse explosion).
//!
//! Aggregated test binary: originally separate test files, merged into submodules (22→4) to cut the number of linked test binaries and speed up builds.
#![allow(clippy::all)]

#[path = "skills/corpse_explosion.rs"]
mod corpse_explosion;
#[path = "skills/cost_multiplier.rs"]
mod cost_multiplier;
#[path = "skills/dual_wield.rs"]
mod dual_wield;
#[path = "skills/gem_quality.rs"]
mod gem_quality;
#[path = "skills/minions.rs"]
mod minions;
#[path = "skills/skill_damage_dps.rs"]
mod skill_damage_dps;
#[path = "skills/spirit_reservation.rs"]
mod spirit_reservation;
#[path = "skills/support_gating.rs"]
mod support_gating;
#[path = "skills/support_gem_count.rs"]
mod support_gem_count;
