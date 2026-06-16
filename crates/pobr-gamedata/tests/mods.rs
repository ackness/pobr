//! 数据加载：词条 / 局部 mod / special / 高精度 / 玩家基础 mod / 覆盖层 / 诅咒优先级。
//!
//! 聚合二进制：原独立测试文件合并为子模块（26→4），减少链接二进制数以加速构建。
#![allow(clippy::all)]

#[path = "mods/load_base_player_mods.rs"]
mod load_base_player_mods;
#[path = "mods/load_curse_priority.rs"]
mod load_curse_priority;
#[path = "mods/load_high_precision_mods.rs"]
mod load_high_precision_mods;
#[path = "mods/load_item_overlay.rs"]
mod load_item_overlay;
#[path = "mods/load_local_mods.rs"]
mod load_local_mods;
#[path = "mods/load_mods.rs"]
mod load_mods;
#[path = "mods/load_special_mods.rs"]
mod load_special_mods;
