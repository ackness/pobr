//! 适配后入库的游戏数据目录（catalog）schema。
//!
//! 这是 PoBR **自有的最小 JSON schema**，由 `pobr-data-adapter` 从 GGG `.dat`
//! 原始导出（pathofexile-dat 产物）解析外键、反范式化后生成，落在仓库
//! `data/<poe_version>/`。运行时由 loader（`pobr-gamedata`）以 serde 加载。
//!
//! 设计目标：与 GGG 原始列名 / PoB 生成 Lua 解耦；只保留计算/显示需要的字段；
//! 稳定字符串 ID；版本可钉、diff 友好（数组按 id 排序）。

use serde::{Deserialize, Serialize};

/// 当前 catalog schema 版本。结构不兼容变更时 +1。
pub const CATALOG_SCHEMA_VERSION: u32 = 1;

/// 数据包信封：描述某个 PoE2 版本下入库了哪些域与语言。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataManifest {
    pub schema_version: u32,
    /// CDN 补丁版本号，如 `4.5.0.3.4`（公开版 0.5.0）。
    pub poe_version: String,
    /// 已生成 i18n 边车的语言标签，如 `["zh-TW"]`（英文为 canonical，不计入）。
    pub languages: Vec<String>,
    /// 已生成的数据域文件名（不含扩展名），如 `["base_items"]`。
    pub domains: Vec<String>,
}

/// 物品基底定义（来自 `BaseItemTypes.dat` + 外键解析）。
///
/// `name` 为英文 canonical；其它语言的名称走 `i18n/<lang>/base_items.json` 边车
/// （`id -> 本地化名称`）。武器/护甲数值（如 PhysicalMin/Max）来自独立的
/// `WeaponTypes` / `ArmourTypes` 表，后续切片接入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseItemDef {
    /// 稳定 ID，即 `.dat` 的 `Id`（如 `Metadata/Items/Weapons/.../FourOneHandAxe1`）。
    pub id: String,
    /// 英文 canonical 名称。
    pub name: String,
    /// 物品类别（解析 `ItemClasses.Id`，如 `One Hand Axe`）。
    pub item_class: String,
    /// 掉落等级。
    pub drop_level: u32,
    /// 物品栏宽 / 高。
    pub width: u8,
    pub height: u8,
    /// 标签（解析 `Tags.Id`，如 `ezomyte_basetype`）。
    pub tags: Vec<String>,
    /// 固有词缀（implicit）的 mod 稳定 ID（解析 `Mods.Id`）。
    pub implicits: Vec<String>,
    /// mod domain（GGG 原始枚举值，用于词缀适用域判定）。
    pub mod_domain: u32,
}
