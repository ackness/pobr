//! StatDescriptions overlay 域 schema（`overlay/stat_descriptions.json`，schema
//! `stat_descriptions/v1`）。
//!
//! 数据来源：vendor PoB2 `Data/StatDescriptions/*.lua`——每个 stat_id 的 canonical
//! 显示文本模板（GGG `stat_descriptions.txt` 编译产物）。由
//! `sync-pob-catalog extract-lua --what stat-descriptions` 确定性抽取生成：luajit
//! 在最小环境下加载描述表、对每个 stat_id 喂代表值（V=1）渲染一段文本，Rust 侧
//! 按 scope 分段 + BTreeMap 字典序序列化保证 byte-stable。
//!
//! 用途（缺口「stat_id → Modifier 第二通道」）：游戏数据里 tree 节点 /
//! 物品隐式 / 宝石词条很多以 **stat_id** 形式给出（而非英文文本）。本表把 stat_id
//! 还原成 canonical 英文文本，再经 `parse_mod_engine` 解析为 Modifier——与现行
//! 英文文本通道并行的第二条注入路径，差异收敛后按域切换。
//!
//! **抽取保真原则**（对齐 [`crate::catalog::stat_map`]）：
//! - 渲染文本**原样**落 JSON，抽取期不做任何语义筛选——「哪条 stat_id 受支持」是
//!   引擎（`parse_mod_engine`）的职责。
//! - 多 scope（root `stat_descriptions` + `passive_skill_*` 等）**各自分段**，不在
//!   抽取期决定 precedence——「child 覆盖 parent」由消费侧 §B 生成器决定，这样
//!   vendor 更新时 overlay 的 drift diff 才逐 scope 可读。
//! - 多 stat 的 compound 描述符**不强解**（一句话挂多个 stat_id），原样保留模板
//!   + member 列表供 §B 处置。
//! - 无可渲染变体（仅负向 limit 等）的 stat_id 记入 `unrendered` 诊断集，不静默丢弃。
//!
//! 本模块只定义 serde 形状，零逻辑。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// StatDescriptions overlay 文档本体（`_meta` 头部之外的平铺部分）。
///
/// 按 scope 名分段（root `stat_descriptions` + 各专域如
/// `passive_skill_stat_descriptions`）；每段独立持有 single / compound /
/// unrendered 三类，precedence 由消费侧决定。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StatDescriptionsDef {
    /// scope 名 → 该 scope 的描述集（BTreeMap 字典序保证确定性）。
    #[serde(default)]
    pub scopes: BTreeMap<String, ScopeDescriptions>,
}

/// 单个 scope（一个 StatDescriptions 文件的 own keys）的描述集。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScopeDescriptions {
    /// 单 stat 描述符 → 渲染出的 canonical 文本行（多行描述按行序保留）。
    /// 这是 §B 生成器的主输入：逐行喂 `parse_mod_engine`。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub single: BTreeMap<String, Vec<String>>,
    /// 多 stat（compound）描述符 → 原样模板 + member 列表（不强解）。
    /// 键 = descriptor 的首个 stat_id（vendor `stats[1]`）。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub compound: BTreeMap<String, CompoundDescription>,
    /// 无可渲染变体的 stat_id（诊断用；§B 跳过，留待人工 overlay 补）。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub unrendered: BTreeSet<String>,
}

/// 多 stat 描述符的原样保留（一句模板文本绑定多个 stat_id）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CompoundDescription {
    /// 该描述符绑定的全部 stat_id（vendor `descriptor.stats`，原序）。
    pub member_stats: Vec<String>,
    /// 原样模板文本（含 `{0}`/`{1}` 占位符与字面 `\n` 换行；§B 自行处置）。
    pub template: String,
}
