//! M0-W2 九表的按域 loader（当前为空壳，与 `pobr_data::catalog` 九表 schema 对应）。
//!
//! 每个子模块在 W2 随 schema 填充实现 `GameData` 上的对应加载方法
//! （走 `base/`/`overlay/` 定位 + overlay merge）；预创建空壳是为了避免
//! 后续并行任务改同一文件冲突。

pub mod base_player_mods;
pub mod character_constants;
pub mod enemy_presets;
pub mod game_constants;
pub mod jewel_radii;
pub mod monster_scaling;
pub mod non_damaging_ailments;
pub mod unarmed_data;
pub mod weapon_types;
