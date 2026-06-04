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

/// Stat 注册表条目（来自 `Stats.dat`）。
///
/// `id` 是 GGG 稳定字符串 stat key（如 `additional_strength`），是 Mods 里
/// `Stat1..4` 整型外键解析后的目标，也是未来 i18n stat 描述的主键。
/// `semantic` / `category` 是 GGG 原始整型枚举（无独立解析表，保留原值）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatDef {
    /// 稳定 stat ID，即 `Stats.dat` 的 `Id`（如 `additional_strength`）。
    pub id: String,
    /// 是否为本地词缀（local，仅作用于所在装备）。
    pub is_local: bool,
    /// GGG 原始 `Semantic` 枚举值（数值正负 / 百分比 / 时长等显示语义）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic: Option<u32>,
    /// GGG 原始 `Category` 枚举值（stat 归类，可空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<u32>,
}

/// 词缀（mod）某个 stat 槽位的掷值区间（来自 `Mods.StatNValue`，形如 `[min, max]`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModStat {
    /// 该槽位作用的 stat 稳定 ID（解析 `StatN` 外键 → `Stats.Id`）。
    pub stat_id: String,
    /// 掷值下界。
    pub min: i64,
    /// 掷值上界。
    pub max: i64,
}

/// 词缀池定义（来自 `Mods.dat` + 外键解析）。
///
/// `name` 为英文 canonical 词缀名（前后缀名，如 `of the Brute`）；其它语言走
/// `i18n/<lang>/mods.json` 边车（`id -> 本地化名称`）。`Stat1..4` + `Stat1Value..4Value`
/// 被合并成 `stats` 数组（解析 stat 外键、跳过空槽）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModDef {
    /// 稳定 ID，即 `Mods.dat` 的 `Id`（如 `Strength1`）。
    pub id: String,
    /// 英文 canonical 词缀名（可空：大量内部 mod 无显示名）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// GGG 原始 `ModType` 枚举值（无独立解析表，保留原值；可空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mod_type: Option<u32>,
    /// mod domain（GGG 原始枚举值，用于词缀适用域判定）。
    pub domain: u32,
    /// GGG 原始 `GenerationType` 枚举值（前缀 / 后缀 / 固有等生成类型；可空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_type: Option<u32>,
    /// 词缀生成等级。
    pub level: u32,
    /// 该词缀作用的 stat 槽位（已合并 Stat 外键 + 掷值区间，跳过空槽）。
    pub stats: Vec<ModStat>,
    /// 标签（解析 `Tags.Id`）。
    pub tags: Vec<String>,
}
