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

/// 技能宝石定义（来自 `SkillGems.dat` + `BaseItemTypes` 外键解析）。
///
/// 宝石**自身无 Id 列**，其身份取自 `BaseItemType` 指向的基底 Id
/// （形如 `Metadata/Items/Gems/SkillGemFireball`）。`name` 走 base_items 域，
/// 此处只存与宝石机制相关的字段。
///
/// TODO（后续切片）：分等级 stat 缩放（GrantedEffectStatSetsPerLevel /
/// GrantedEffectsPerLevel 的 cost / cooldown / 伤害进度）尚未接入；
/// `GemEffects` FK 指向的 `GemEffects` 表当前 pipeline 未导出，
/// 故宝石→授予效果的直接连边暂缺，等该表导出后补 `granted_effects: Vec<String>`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillGemDef {
    /// 稳定 ID，取自 `BaseItemType` 基底的 `Id`。
    pub id: String,
    /// 宝石类型（GGG 原始枚举：0=主动技能，1=辅助），保留原值便于排查。
    pub gem_type: Option<u32>,
    /// 宝石颜色（GGG 原始枚举：1=红/力，2=绿/敏，3=蓝/智，4=白等）。
    pub gem_colour: Option<u32>,
    /// 使用所需最小角色等级。
    pub min_level_req: u32,
    /// 力量需求百分比（属性需求权重）。
    pub str_pct: u32,
    /// 敏捷需求百分比。
    pub dex_pct: u32,
    /// 智慧需求百分比。
    pub int_pct: u32,
    /// 是否为辅助宝石（由 `GemType == 1` 判定）。
    pub is_support: bool,
}

/// 授予效果定义（来自 `GrantedEffects.dat` + 外键解析）。
///
/// 每个宝石/物品最终授予一个或多个 `GrantedEffect`；主动技能效果会关联到一条
/// `ActiveSkills` 记录（显示名 / 技能类型）。本切片只取身份 + 主动技能链接 +
/// 施放时间 + 允许的主动技能类型枚举。
///
/// TODO（后续切片）：`StatSet` / `CostTypes` / 分等级缩放
/// （`GrantedEffectsPerLevel` 的 cost / cooldown / attack time）尚未接入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantedEffectDef {
    /// 稳定 ID，即 `GrantedEffects.Id`（如 `FireballPlayer`）。
    pub id: String,
    /// 是否为辅助效果。
    pub is_support: bool,
    /// 关联的主动技能 Id（解析 `ActiveSkills.Id`；辅助效果为 None）。
    pub active_skill: Option<String>,
    /// 施放/吟唱时间（毫秒）。原始值为 0（瞬发/辅助）时归一化为 None。
    pub cast_time: Option<u32>,
    /// 允许作用的主动技能类型（GGG 原始枚举值；当前无导出的类型名查表）。
    pub allowed_active_skill_types: Vec<u32>,
}
