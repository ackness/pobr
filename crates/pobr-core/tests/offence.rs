//! 进攻侧：伤害 / 暴击 / 转换 / 缩放 / 触发 / 技能机制 / DPS。
//!
//! 聚合二进制：原独立测试文件合并为子模块，减少链接二进制数（53→8）以加速构建。
//! 各子模块即 `tests/offence/<name>.rs`，测试用例与断言逐一保留。
#![allow(clippy::all)]

#[path = "offence/crit.rs"]
mod crit;
#[path = "offence/crit_pass.rs"]
mod crit_pass;
#[path = "offence/crossbow_reload.rs"]
mod crossbow_reload;
#[path = "offence/damage_components.rs"]
mod damage_components;
#[path = "offence/damage_conversion.rs"]
mod damage_conversion;
#[path = "offence/damage_double_dip.rs"]
mod damage_double_dip;
#[path = "offence/lucky_can_deal.rs"]
mod lucky_can_deal;
#[path = "offence/pool_damage.rs"]
mod pool_damage;
#[path = "offence/scaled_damage.rs"]
mod scaled_damage;
#[path = "offence/skill_mechanics.rs"]
mod skill_mechanics;
#[path = "offence/skill_use_time.rs"]
mod skill_use_time;
#[path = "offence/taken_as.rs"]
mod taken_as;
#[path = "offence/trigger.rs"]
mod trigger;
