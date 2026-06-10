//! SkillStatMap overlay 域 schema（`overlay/skill_stat_map.json`，schema
//! `skill_stat_map/v1`）。
//!
//! 数据来源：vendor PoB2 `Data/SkillStatMap.lua`（954 条全局 stat → modifier
//! 构造器映射）+ `Data/Skills/{act_*,sup_*,other}.lua` 各 statSet 的 `statMap`
//! 字段（per-set 覆盖；minion/spectre 留 M5a）。由
//! `sync-pob-catalog extract-lua --what stat-map` 确定性抽取生成。
//!
//! **抽取保真原则**（M1 蓝图 T2.1）：mod 构造器的 tags / 嵌套表**原样**纯表化
//! 落 JSON，抽取期不做任何语义筛选——「哪些 tag/字段受支持」的判定是引擎
//! （`pobr-core::rules::stat_map_engine`）的职责。这样 vendor 更新时 overlay
//! 的 drift diff 才有意义。遇 Lua 函数值等不可序列化字段时整条标
//! `_unextractable: true` 上报（引擎归 Unsupported），不静默丢弃。
//!
//! merge 语义（消费侧，vendor `Modules/CalcActiveSkill.lua:112` 逐字对齐）：
//! `注入值 = entry.value or stat值 × (entry.mult or 1) × scalar / (entry.div or 1) + (entry.base or 0)`
//! （group 元素用 group 级参数替代 entry 级参数；scalar M1 固定 1.0）。
//! 本模块只定义 serde 形状，零逻辑。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 任意 Lua 纯值的忠实 JSON 表示（tag 字段 / mod 构造器字面 value 用）。
///
/// `untagged`：序列化即裸 JSON 值；对象键经 [`BTreeMap`] 字典序排序保证
/// 确定性。Lua 函数值不可表示——抽取器遇到时整条标 `_unextractable`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StatMapValue {
    /// 布尔（如 flag 构造器的 `true`、tag 的 `limitTotal = true`）。
    Bool(bool),
    /// 数值（Lua number 一律 f64）。
    Number(f64),
    /// 字符串（如 tag 的 `var` / `stat` 名）。
    Text(String),
    /// 数组（仅连续整数键 1..n 的 Lua 表，如 `DistanceRamp` 的 ramp 点列）。
    List(Vec<StatMapValue>),
    /// 对象（带字符串键的 Lua 表，键按字典序）。
    Table(BTreeMap<String, StatMapValue>),
}

/// 单个 mod 构造器捕获（vendor `mod()` / `flag()` / `skill()`）或 group。
///
/// vendor 三个构造器同形（`Modules/Data.lua` `makeSkillMod` 族）：
/// `flag(name, ...)` = `mod(name, "FLAG", true, 0, 0, ...)`；
/// `skill(key, value, ...)` = `mod("SkillData", "LIST", {key, value}, 0, 0, ...)`。
/// 抽取器按调用的构造器记 [`Self::kind`]，便于引擎免重推断。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatMapMod {
    /// 元素种类：`"mod"` / `"flag"` / `"skill_data"` / `"group"`
    /// （group = vendor 无 name 的嵌套 mod 列表，携带组级 merge 参数）。
    pub kind: String,
    /// PoB2 内部 ModName **原样保留**（如 `FireDamage` / `Speed` / `CritChance`）；
    /// 翻译到 PoBR ModName 在引擎的 Rust 常量表（框架语义 L4）。group 无 name。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 聚合类型原文：`BASE` / `INC` / `MORE` / `FLAG` / `LIST` / `OVERRIDE`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mod_type: Option<String>,
    /// 构造器字面 value（多数 mod 为 `nil`——值由 merge 公式运行时填充；
    /// flag 恒 `true`；skill_data 为 `{key, value}` 表；LIST mod 为嵌套表）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<StatMapValue>,
    /// ModFlag token 名列表（stub 自映射保证是名字；`0` → 空）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    /// KeywordFlag token 名列表（同上；`OR64(a, b)` 展开为多名）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keyword_flags: Vec<String>,
    /// tag 表列表，**原样纯表化**（不筛选；引擎按 tag `type` 分批支持）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<BTreeMap<String, StatMapValue>>,
    /// group 级 scalar 变量名（vendor `checkForScalarMultiplier` 反查
    /// `Multiplier:<scalar>`；M1 引擎固定 scalar=1.0 并把含 scalar 条目归
    /// Unsupported）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scalar: Option<String>,
    /// group 级 merge 参数（仅 `kind == "group"`；替代 entry 级同名参数）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub div: Option<f64>,
    /// 见 [`Self::div`]。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mult: Option<f64>,
    /// 见 [`Self::div`]。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<f64>,
    /// group 内嵌套 mod 列表（仅 `kind == "group"`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mods: Vec<StatMapMod>,
}

/// 一条 stat → modifier 映射条目（vendor statMap 表的一个值）。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct StatMapEntry {
    /// entry 级 merge 参数 `div`（如 `total_cast_time_+_ms` 的 1000，毫秒→秒）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub div: Option<f64>,
    /// entry 级 merge 参数 `mult`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mult: Option<f64>,
    /// entry 级 merge 参数 `base`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<f64>,
    /// entry 级 value 覆盖（如 `global_bleed_on_hit` 的 100——忽略 stat 值恒注入 100）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// 技能 flag 名（如 `skill_can_fire_arrows` → `"arrow"`；由 PoB2 statSet
    /// flags 路径消费而非 merge 公式，引擎第一批归 Unsupported）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_flag: Option<String>,
    /// mod 构造器 / group 列表（vendor 表的数组部分，顺序保留）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mods: Vec<StatMapMod>,
    /// 抽取失真标记：条目含 Lua 函数值等不可序列化字段（引擎归 Unsupported）。
    #[serde(
        default,
        rename = "_unextractable",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub unextractable: bool,
}

/// `overlay/skill_stat_map.json` 顶层（消费侧视角：`_meta` 头部为生成溯源
/// 信息，serde 默认忽略未知字段，消费侧只取 `global` / `per_stat_set`）。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SkillStatMapDef {
    /// 全局映射：stat id → 条目（vendor `Data/SkillStatMap.lua`，键字典序）。
    pub global: BTreeMap<String, StatMapEntry>,
    /// per-statSet 覆盖：granted effect id → statSet 序号（十进制字符串，
    /// vendor `statSets` 数组 1-based 下标）→ stat id → 条目。
    /// 查找语义：per-set 命中优先，miss 落回 [`Self::global`]。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub per_stat_set: BTreeMap<String, BTreeMap<String, BTreeMap<String, StatMapEntry>>>,
}
