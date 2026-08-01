//! 物品编辑态四张 overlay 表 schema：
//!
//! | 表 | 路径 | vendor 来源 |
//! |---|---|---|
//! | `mod_scalability.json` | overlay/ | `Data/ModScalability.lua`（15037 行，纯 `return {...}`） |
//! | `catalysts.json`       | overlay/ | `Classes/Item.lua:14-29` 三个 local 表字面量 |
//! | `runes.json`           | overlay/ | `Data/ModRunes.lua`（纯 `return {...}`） |
//! | `uniques.json`         | overlay/ | `Data/Uniques/*.lua`（raw 文本块数组）+ `Special/race` |
//!
//! 全部由 `sync-pob-catalog extract-lua --what mod-scalability|catalysts|runes|uniques`
//! 确定性抽取生成（luajit 执行 vendor 序列化，产物 byte-stable、`_meta`
//! 记 vendor commit、禁手改）。本模块只定义 serde 形状，零逻辑、零 I/O。
//!
//! 消费侧：
//! - `mod_scalability` / `catalysts` → `pobr-core::apply_range` 取值引擎
//!   经 RuleSet `ItemRules` 注入；
//! - `runes` / `uniques` → pobr-item 编辑态，按需单独加载，不进 ItemRules。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

// mod_scalability —— `{range:x}` 词条取值的可缩放性 + 格式换算表

/// 词条模板中一个数值槽的缩放规则（对应 vendor 条目数组的一项）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModScalabilitySlotDef {
    /// 该数值槽是否可随 `{range:x}` / catalyst 缩放（`isScalable`）。
    #[serde(default)]
    pub is_scalable: bool,
    /// 数值格式换算枚举（`formats`，如 `divide_by_one_hundred` /
    /// `per_minute_to_per_second`；消费侧对未知 format 载入期告警）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub formats: Vec<String>,
}

/// 一条 `#`-模板化词条的缩放规则（vendor key = 数值已替换为 `#` 的词条文本）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModScalabilityEntryDef {
    /// `#`-化词条文本（如 `# Armour per 2 Strength`）。
    pub template: String,
    /// 逐数值槽的缩放规则（与模板中 `#` 出现序一一对应）。
    pub slots: Vec<ModScalabilitySlotDef>,
}

/// `overlay/mod_scalability.json` 顶层（消费侧忽略 `_meta`）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ModScalabilityDef {
    /// 条目列表，按 `template` 升序。
    pub entries: Vec<ModScalabilityEntryDef>,
}

// catalysts —— 催化剂品质标签匹配表

/// 一种催化剂（vendor `Classes/Item.lua:14-29` 三个平行数组的按 index 合并）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalystDef {
    /// 1-based 序号（= XML `Catalyst` 属性值；vendor `catalystList` 下标）。
    pub id: u8,
    /// 名称前缀（`catalystList`，如 `Carapace`）。
    pub name: String,
    /// 描述词（`catalystDescriptorList`，如 `Defence`；物品文本
    /// `Quality (<descriptor> Modifiers)` 反查 id 用）。
    pub descriptor: String,
    /// 匹配的 mod tag 集（`catalystTags`；`getCatalystScalar` 按
    /// `catalystTags[id] ∩ mod.modTags ≠ ∅` 给 `(100+quality)/100`）。
    pub tags: Vec<String>,
}

/// `overlay/catalysts.json` 顶层（消费侧忽略 `_meta`）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CatalystsDef {
    /// 催化剂列表，按 `id` 升序（12 条）。
    pub catalysts: Vec<CatalystDef>,
}

// runes —— 符文 / 魂核镶嵌词条表

/// 符文在某一槽类上的词条组（vendor `ModRunes.lua` 二级表）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuneSlotDef {
    /// 类别字面量（`type`：`Rune` / `SoulCore`）。
    pub kind: String,
    /// 已渲染词条行（vendor 表的数组部分）。
    pub lines: Vec<String>,
    /// 排序权重（`rank`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rank: Vec<f64>,
    /// 词条 statOrder（与 `lines` 同序，编辑态合并排序用）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stat_order: Vec<f64>,
}

/// 一种符文 / 魂核（vendor key = 符文名）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuneDef {
    /// 符文名（如 `Desert Rune` / `Hayoxi's Soul Core of Heatproofing`）。
    pub name: String,
    /// 槽类 → 词条组（键如 `weapon`/`helmet`/`boots`，BTreeMap 保证键序）。
    pub slots: BTreeMap<String, RuneSlotDef>,
}

/// `overlay/runes.json` 顶层（消费侧忽略 `_meta`）。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RunesDef {
    /// 符文列表，按 `name` 升序。
    pub runes: Vec<RuneDef>,
}

// uniques —— 传奇物品 raw 文本块 + 预解析索引（双层）

/// 一件传奇（双层：`raw` 保留 vendor 原始文本块逐字节，索引列只做最小
/// 预解析——name/base/variants/league/source；词条模板行解析由 pobr-item
/// 运行时做）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UniqueDef {
    /// 物品名（raw 块第 1 行）。
    pub name: String,
    /// 基底名（raw 块第 2 行）。
    pub base: String,
    /// 来源文件域（vendor `Data/Uniques/<item_type>.lua` 的 `<item_type>`，
    /// 如 `amulet`；`Special/race` 记为 `race`）。
    pub item_type: String,
    /// vendor 原始文本块（含 `Variant:`/`League:`/`{tags:...}` 标注，逐字节保留）。
    pub raw: String,
    /// `Variant:` 行列表（预解析索引；按出现序）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<String>,
    /// `League:` 行值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub league: Option<String>,
    /// `Source:` 行值（多行时取首行，完整内容看 `raw`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// `Upgrade:` 行值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upgrade: Option<String>,
}

/// `overlay/uniques.json` 顶层（消费侧忽略 `_meta`）。
///
/// 范围注：`Data/Uniques/Special/Generated.lua`（程序化生成，依赖运行时
/// itemMods）与 `Special/New.lua`（未定稿池）不在抽取范围，见 `_meta` 注记。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UniquesDef {
    /// 传奇列表，按 `(item_type, name, 出现序)` 升序。
    pub uniques: Vec<UniqueDef>,
}
