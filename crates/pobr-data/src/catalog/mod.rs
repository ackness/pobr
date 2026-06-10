//! 适配后入库的游戏数据目录（catalog）schema。
//!
//! 这是 PoBR **自有的最小 JSON schema**，由 `pobr-data-adapter` 从 GGG `.dat`
//! 原始导出（pathofexile-dat 产物）解析外键、反范式化后生成，落在仓库
//! `data/<poe_version>/`（三层布局：`base/` 全自动再生、`overlay/` vendor 抽取、
//! `generated/` 确定性缓存，见架构文档 20 §1 P1）。运行时由 loader
//! （`pobr-gamedata`）以 serde 加载。
//!
//! 设计目标：与 GGG 原始列名 / PoB 生成 Lua 解耦；只保留计算/显示需要的字段；
//! 稳定字符串 ID；版本可钉、diff 友好（数组按 id 排序）。
//!
//! 模块按数据域内聚拆分；所有类型经此处 `pub use` 全量 re-export，
//! 外部一律以 `pobr_data::catalog::X` 路径引用（拆分不破坏既有路径）。

pub mod items;
pub mod manifest;
pub mod mods;
pub mod skills;
pub mod tree;

// ---- M0-W2 九表 schema 空壳（防并行冲突预创建，W2 填充类型）----
pub mod base_player_mods;
pub mod character_constants;
pub mod enemy_presets;
pub mod game_constants;
pub mod jewel_radii;
pub mod monster_scaling;
pub mod non_damaging_ailments;
pub mod unarmed_data;
pub mod weapon_types;

// ---- M0-W3 注入管道：calc 消费的运行时常量包 ----
pub mod runtime;

// ---- M0-W4d 小查表 overlay schema（取整精度例外表 + 局部词条白名单）----
pub mod high_precision_mods;
pub mod local_mods;

pub use items::{ArmourBaseStats, BaseItemDef, WeaponBaseStats};
pub use manifest::{CATALOG_SCHEMA_VERSION, DataManifest, DomainSections};
pub use mods::{ModDef, ModStat, StatDef};
pub use runtime::RuntimeConstants;
pub use skills::{
    CostTypeDef, GrantedEffectDef, SkillDamageStat, SkillGemDef, SkillLevelDef, SkillStatSetDef,
    SkillStatSetLevel,
};
pub use tree::{PassiveAscendancy, PassiveClass, PassiveNodeDef, PassiveNodeKind, PassiveTreeMeta};
pub use unarmed_data::{UnarmedDataTable, UnarmedWeaponDef};
pub use weapon_types::{WeaponTypeDef, WeaponTypeTable};
