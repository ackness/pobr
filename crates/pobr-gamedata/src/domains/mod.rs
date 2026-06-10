//! M0 按域 loader：W2 九张常量表（`base/`）+ W4d 两张小查表（`overlay/`），
//! 与 `pobr_data::catalog` 各域 schema 对应。
//!
//! 每个子模块实现 `GameData` 上的对应加载方法（base 域走 `base/` 优先、
//! 版本根回退定位；overlay 域恒走 `overlay/` 定位）。

pub mod base_player_mods;
pub mod character_constants;
pub mod enemy_presets;
pub mod game_constants;
pub mod jewel_radii;
pub mod monster_scaling;
pub mod non_damaging_ailments;
pub mod unarmed_data;
pub mod weapon_types;

// ---- M0-W4d 小查表（overlay 层）----
pub mod high_precision_mods;
pub mod local_mods;
