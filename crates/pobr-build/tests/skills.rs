//! 技能 / DPS / 机制端到端(伤害 / 双持 / 成本 / 召唤 / 预留 / 品质 / gating / 尸爆)。
//!
//! 聚合二进制：原独立测试文件合并为子模块（22→4），减少链接二进制数以加速构建。
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
