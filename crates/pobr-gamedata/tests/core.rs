//! 数据加载：角色常量 / 游戏常量 / 配置项 / stat set 入库。
//!
//! 聚合二进制：原独立测试文件合并为子模块（26→4），减少链接二进制数以加速构建。
#![allow(clippy::all)]

#[path = "core/full_stat_ingestion.rs"]
mod full_stat_ingestion;
#[path = "core/load_character_constants.rs"]
mod load_character_constants;
#[path = "core/load_config_options.rs"]
mod load_config_options;
#[path = "core/load_game_constants.rs"]
mod load_game_constants;
#[path = "core/multi_stat_sets.rs"]
mod multi_stat_sets;
#[path = "core/version_and_patch.rs"]
mod version_and_patch;
