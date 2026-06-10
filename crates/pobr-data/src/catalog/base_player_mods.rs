//! 玩家固有基线 mod 域 schema（`base/base_player_mods.json`）。
//!
//! 对应 PoB2 玩家 modDB 初始化注入的固有基线 mod：
//! - `vendor/PathOfBuilding-PoE2/src/Modules/CalcSetup.lua:19-105`（`initModDB`，充能上限等）；
//! - `vendor/PathOfBuilding-PoE2/src/Modules/CalcSetup.lua:608-678`（initEnv 玩家基线段）。
//!
//! **搬迁不变式（P8）**：本表只收录 pobr 现有 Rust 代码已有数值的条目，且 JSON 值必须
//! 与 Rust 准源逐值相等；vendor 有而 pobr 没有的条目（如 `DotMultiplier`/`MaximumRage`/
//! `ActiveTrapLimit`/Tailwind 段等约 60 条）**不入本表**，留待后续行为对齐 commit 补充。
//! 各条目的 Rust 准源与 vendor 行号见 `data/<版本>/base/base_player_mods.json` 生成来源
//! （逐条对照测试：`crates/pobr-gamedata/tests/load_base_player_mods.rs`）。
//!
//! 类型复用说明：`mod_type` 直接复用 [`crate::modifier::ModType`]（已可 serde）；
//! `flags`/`keyword_flags` 以位值（u64）序列化，位定义对齐
//! [`crate::modifier::ModFlags`] / [`crate::modifier::KeywordFlags`]；tag 因
//! pobr-core 的 `ModTag` 不在本 crate 且含非序列化变体，此处定义可序列化的最小子集
//! [`BasePlayerModTag`]。

use serde::{Deserialize, Serialize};

use crate::modifier::ModType;

/// 玩家固有基线 mod 条目（加载后由 setup/编排层转为 `Modifier` 注入玩家 ModDb）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BasePlayerModDef {
    /// 稳定条目 ID（小写下划线；用于归因 source id 与逐条对照测试定位）。
    pub id: String,
    /// ModName（计算内部稳定名，如 `MaximumLife` / `FireResistance`）。
    ///
    /// 注意：部分条目与 vendor 命名有出入（pobr 用 `FireResistance`，vendor 用
    /// `FireResist`；pobr 用 `MaximumLife`，vendor 用 `Life`）——按搬迁不变式保留
    /// pobr 现有命名。
    pub mod_name: String,
    /// modifier 类型（`Base` / `Inc` / `More` / `Flag` / `Override` / `List`）。
    pub mod_type: ModType,
    /// 数值（`Flag` 条目约定 `1.0` 为真）。
    pub value: f64,
    /// ModFlags 位值（对齐 [`crate::modifier::ModFlags`]；0 = 无限定，序列化时省略）。
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub flags: u64,
    /// KeywordFlags 位值（对齐 [`crate::modifier::KeywordFlags`]；0 = 无限定，序列化时省略）。
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub keyword_flags: u64,
    /// tag 列表（空 = 无条件生效，序列化时省略）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<BasePlayerModTag>,
}

/// 基线 mod 的可序列化 tag（pobr-core `ModTag` 的最小 serde 子集 + vendor 扩展字段）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BasePlayerModTag {
    /// 按某变量数量线性缩放（PoB2 `Multiplier` tag，对应 pobr-core `ModTag::Multiplier`）。
    ///
    /// 有效值 = `multiplier(var) / div × value + base`。`base` 为 vendor 扩展常量项
    /// （如 `CalcSetup.lua:615` `Life BASE 12 {Multiplier, var=Level, base=16}`），
    /// pobr-core `ModTag::Multiplier` 暂无此字段——当前由 `character.rs` 公式等价
    /// 实现（`12×Level+16`），消费侧接线属后续行为对齐工作。
    Multiplier {
        var: String,
        /// 每多少单位缩放一次（PoB2 `div`，缺省 1）。
        #[serde(default = "default_div")]
        div: f64,
        /// 缩放次数上限（PoB2 `limit`；无上限时省略）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<f64>,
        /// 与缩放无关的附加常量项（vendor `base`；无时省略）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<f64>,
    },
    /// 布尔条件门控（PoB2 `Condition` tag，对应 pobr-core `ModTag::Condition`）。
    Condition {
        var: String,
        /// 取反（PoB2 `neg`；false 时省略）。
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        negated: bool,
    },
}

/// `div` 缺省值（PoB2 Multiplier tag 无除数时为 1）。
fn default_div() -> f64 {
    1.0
}

/// serde 跳过零位值（diff 友好）。
fn is_zero_u64(v: &u64) -> bool {
    *v == 0
}
