//! 取整精度例外表域 schema（`overlay/high_precision_mods.json`）。
//!
//! 数据来源（vendor commit `2df5a74`，见 `vendor/.pob2-version.txt`）：
//! - `src/Modules/Data.lua:413` `data.defaultHighPrecision = 1`；
//! - `src/Modules/Data.lua:415-530` `data.highPrecisionMods`（38 条
//!   `mod name → mod type → 精度位数`）。
//!
//! vendor 消费点（本表的目标接线位，**pobr 当前均未实现**）：
//! - `src/Classes/ModStore.lua:45-81` `ScaleAddMod`：缩放后数值的取整精度——
//!   命中本表用 `floor(v·10^p)/10^p`，未命中但缩放出小数用 `defaultHighPrecision`，
//!   否则 `modf(round(v, 2))` 取整数部分；
//! - `src/Classes/ModList.lua:118-147` `MoreInternal`：MORE 连乘逐 modName
//!   取整——命中本表 `floor(result·10^p)/10^p`，未命中 `round(modResult, 2)`。
//!
//! pobr 现状：
//! - `pobr-core::mod_db::round_more` 固定 `round(·, 2)`，**无例外查表分支**
//!   （与 vendor `MoreInternal` 的默认分支逐值一致）；
//! - ScaleAddMod 原语整体未实现（audits/rearchitecture-2026-06-10/10-mod-system.md
//!   Gap 6）。因此本表**当前零消费方、零 parity 影响**——按先落库，
//!   待 ScaleAddMod / MORE 例外分支落地时作为注入数据接线。
//! - `more_default_round_decimals` 是唯一有 pobr 准源的字段（= round_more 的 2）。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// `overlay/high_precision_mods.json` 顶层（单对象域）。
///
/// 精度语义：`p` 位数即保留 `p` 位小数（`floor(v × 10^p) / 10^p`，vendor 用
/// `floor` 而非四舍五入）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighPrecisionModsDef {
    /// 未命中例外表、但缩放结果含小数时的默认精度位数
    /// （vendor `Data.lua:413` `defaultHighPrecision = 1`；仅 ScaleAddMod 消费）。
    pub default_high_precision: u32,
    /// MORE 连乘逐 modName 的默认取整小数位
    /// （vendor `ModList.lua:144` `round(modResult, 2)` 的 2；
    /// pobr 准源：`pobr-core::mod_db::round_more` 固定 2 位，逐值一致）。
    pub more_default_round_decimals: u32,
    /// 精度例外表：`mod name → (mod type → 精度位数)`。
    ///
    /// mod type 取 vendor 字面量 `"BASE"` / `"MORE"`（与 pobr `ModType` 的
    /// 序列化名对齐留待接线波次裁决，本表忠实转录 vendor 键）。
    pub mods: BTreeMap<String, BTreeMap<String, u32>>,
}
