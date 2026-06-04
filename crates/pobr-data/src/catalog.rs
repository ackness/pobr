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

/// 被动天赋节点的种类。
///
/// 源自 GGG 官方树导出（`poe2-skilltree-export/data.json`）的节点布尔标志：
/// `isKeystone` / `isNotable` / `isMastery` / `isJewelSocket` / `isAscendancyStart`，
/// 否则为 [`PassiveNodeKind::Normal`]（小天赋）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PassiveNodeKind {
    /// 小天赋（普通属性节点）。
    Normal,
    /// 大天赋（notable）。
    Notable,
    /// 基石（keystone）。
    Keystone,
    /// 精通节点（mastery）。
    Mastery,
    /// 珠宝插槽。
    JewelSocket,
    /// 飞升起始节点。
    AscendancyStart,
}

/// 被动天赋树节点定义（来自 GGG 官方树导出 `data.json` 的 `nodes`）。
///
/// 计算内部只用稳定 ID：`id` 为 GGG 的字符串 slug（如
/// `passive_keystone_avatar_of_fire`），`skill` 为数值 skill id（树连线 / Build Code
/// 引用的稳定数值键）。`stats` 是节点授予的英文词条文本行（PoB 兼容解析的输入）。
/// `connections` 为该节点的出边目标 `skill` id（无向树用出边即可重建邻接）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveNodeDef {
    /// 数值 skill id（GGG `nodes` 的 map key / `skill` 字段）。树连线、Build Code 用此引用。
    pub skill: u32,
    /// 字符串 slug（GGG `id` 字段，如 `passive_keystone_avatar_of_fire`）。
    pub id: String,
    /// 节点名称（英文 canonical）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 节点种类。
    pub kind: PassiveNodeKind,
    /// 节点授予的词条文本行（英文 canonical；i18n 边车后续切片）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stats: Vec<String>,
    /// 所属节点组（GGG `group`，用于坐标/布局；计算无关，保留以便和 PoB2 对比）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u32>,
    /// 在 orbit 上的环号（GGG `orbit`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orbit: Option<u32>,
    /// 在 orbit 上的角度槽位（GGG `orbitIndex`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orbit_index: Option<u32>,
    /// 出边目标节点的 `skill` id（GGG `out`，已从字符串 key 转为数值）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connections: Vec<u32>,
    /// 所属飞升（GGG `ascendancyId`，如 `Warrior3`）；主树节点为 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ascendancy_id: Option<String>,
}

/// 某个职业的飞升摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveAscendancy {
    /// 飞升稳定 ID（与节点 `ascendancy_id` 对应，如 `Warrior3`）。
    pub id: String,
    /// 飞升名称（英文 canonical，如 `Smith of Kitava`）。
    pub name: String,
}

/// 某个职业的摘要（基础属性 + 飞升列表）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveClass {
    /// 职业名称（英文 canonical，如 `Warrior`）。
    pub name: String,
    /// 基础力量 / 敏捷 / 智慧。
    pub base_str: i32,
    pub base_dex: i32,
    pub base_int: i32,
    /// 该职业的飞升摘要（无名占位项已过滤）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ascendancies: Vec<PassiveAscendancy>,
}

/// 被动天赋树的元数据摘要（职业 / 飞升 / 树名）。
///
/// orbit 半径 / 每环槽位数（PoB 的 `constants`）在当前 GGG PoE2 导出中**未以独立
/// `constants` 块给出**（坐标直接落在节点 `x`/`y`），故此切片不收录——见 TODO。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveTreeMeta {
    /// 树标识（GGG `tree`，如 `Default`）。
    pub tree: String,
    /// 职业 + 飞升摘要（按职业名排序）。
    pub classes: Vec<PassiveClass>,
}
