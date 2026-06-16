//! 数据加载：技能宝石 / 覆盖 / buff / 触发 / 召唤物 / 怪物缩放 / 敌方预设 / 异常。
//!
//! 聚合二进制：原独立测试文件合并为子模块（26→4），减少链接二进制数以加速构建。
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
