//! 数据加载：基底物品 / 武器类型 / 徒手 / 珠宝半径 / 天赋树。
//!
//! 聚合二进制：原独立测试文件合并为子模块（26→4），减少链接二进制数以加速构建。
#![allow(clippy::all)]

#[path = "items/load_base_items.rs"]
mod load_base_items;
#[path = "items/load_jewel_radii.rs"]
mod load_jewel_radii;
#[path = "items/load_passive_tree.rs"]
mod load_passive_tree;
#[path = "items/load_unarmed_data.rs"]
mod load_unarmed_data;
#[path = "items/load_weapon_types.rs"]
mod load_weapon_types;
