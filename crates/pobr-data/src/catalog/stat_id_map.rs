//! StatId→Modifier 映射表 overlay 域 schema（`overlay/stat_id_map.json`，schema
//! `stat_id_map/v1`）。
//!
//! M6 E/F「stat_id → Modifier 第二通道」的产物（段 B）：把段 A 抽取的
//! `stat_descriptions.json`（stat_id → canonical 文本）逐条喂 `parse_mod_engine`，
//! 把能解析的 stat_id 固化成 modifier 模板。由 `sync-pob-catalog gen-stat-id-map`
//! 离线生成（消费两份 overlay：stat_descriptions + mod_parser_rules）。
//!
//! 运行期用途：游戏数据给出 `(stat_id, 原始值)` 时，查本表拿预解析的 modifier
//! 结构，按 `运行期值 = 原始值 × coefficient` 注入——免去文本往返（文本渲染要
//! luajit，仅构建期可用）。这是与英文文本通道并行的第二条注入路径。
//!
//! **设计约定**：
//! - 模板把**结构**（name / mod_type / tags / flags）与**系数**（coefficient，
//!   段 A 喂 V=1 解析得的每单位值，线性通用词条恒 1）分列——结构供 dual-run
//!   逐字段比较，系数供运行期缩放。tag 用 `pobr_core::mod_parser::canonical_tags`
//!   的单源串（文档禁两套序列化）；空串 = 无 tag（常见通用词条即此）。
//! - 多 scope 各自分段（对齐 [`crate::catalog::stat_descriptions`]），precedence
//!   交消费侧。
//! - 无法解析的 stat_id 记入 `unsupported`（通道回退文本 / special），不静默丢弃。
//!
//! 本模块只定义 serde 形状，零逻辑。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// StatId→Modifier 映射文档本体（`_meta` 头部之外的平铺部分）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StatIdMapDef {
    /// scope 名 → 该 scope 的映射（BTreeMap 字典序保证确定性）。
    #[serde(default)]
    pub scopes: BTreeMap<String, ScopeStatIdMap>,
}

/// 单个 scope 的 stat_id → modifier 模板映射。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScopeStatIdMap {
    /// stat_id → 解析出的 modifier 模板（该 stat_id 全部文本行的 mod 顺序合并）。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mapped: BTreeMap<String, Vec<StatIdModTemplate>>,
    /// 文本无法解析的 stat_id（通道回退文本 / special 处置）。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub unsupported: BTreeSet<String>,
}

/// 单条 modifier 模板（结构 + 系数分列）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StatIdModTemplate {
    /// ModName（PoBR 内部稳定 ID）。
    pub name: String,
    /// 聚合类型：`Base` / `Inc` / `More` / `Flag` / `Override` / `List`。
    pub mod_type: String,
    /// 段 A 喂 V=1 解析得的每单位值（运行期 modifier 值 = 原始 stat 值 ×
    /// coefficient；线性通用词条恒 1.0）。非数值载荷时为 `None`，见 `value_kind`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coefficient: Option<f64>,
    /// 非数值载荷标记（`flag:true` / `text:...` / `nested`）；数值时省略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_kind: Option<String>,
    /// canonical tag 串（`pobr_core::mod_parser::canonical_tags`；空 = 无 tag）。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tags: String,
    /// ModFlag 位（0 = 无）。
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub flags: u64,
    /// KeywordFlag 位（0 = 无）。
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub keyword_flags: u64,
}

fn is_zero_u64(v: &u64) -> bool {
    *v == 0
}
