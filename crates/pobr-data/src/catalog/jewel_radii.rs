//! 范围珠宝环形档域 schema（`base/jewel_radii.json`）。
//!
//! 数据来源：
//! - 档位表：vendor PoB2 `src/Modules/Data.lua:595-611` `data.jewelRadii["0_1"]`
//!   （4 个具名档 Small/Medium/Large/Very Large + 8 个 `inner > 0` 的 Variable 环形档）；
//! - 距离乘数：vendor PoB2 `src/Data/Misc.lua:36`
//!   `gameConstants["PassiveTreeJewelDistanceMultiplier"] = 1.2`（转录自 `GameConstants.dat`）。
//!
//! pobr 现有 Rust 准源：`crates/pobr-tree/src/radius_jewel.rs` 的
//! `PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER` 与 `JEWEL_RADIUS_{SMALL,MEDIUM,LARGE,VERY_LARGE}`
//! （4 个具名档的 outer 与乘数已逐值一致）；`inner`/`colour` 与 8 个 Variable 档为
//! vendor-only 补全（pobr-tree first pass 用 `JewelRadius::Custom` 兜 Variable，不带 inner 语义）。
//!
//! 距离判定语义（对照 PoB2 `Modules/Data.lua:584-586` `setJewelRadiiGlobally`）：
//! 节点落入环形档 ⇔ `inner² × m² <= dx² + dy² <= outer² × m²`，其中
//! `m = distance_multiplier`；即 inner/outer 均为未乘缩放因子的基础半径（tree units）。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// `base/jewel_radii.json` 顶层：距离乘数 + 按树版本的环形档位表。
///
/// 树版本键形如 `"0_1"`（major_minor）；运行时按 PoB2 `setJewelRadiiGlobally`
/// 语义选取 `<=` 目标树版本中最新的一组（当前数据只有 `0_1` 一组）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JewelRadiiDef {
    /// 天赋树珠宝距离缩放因子（`PassiveTreeJewelDistanceMultiplier`，当前 1.2）。
    ///
    /// 来源：`Data/Misc.lua:36`（GameConstants.dat）。pobr 准源：
    /// `pobr-tree::PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER`（逐值一致）。
    pub distance_multiplier: f64,
    /// `树版本 -> 环形档位数组`（保持 vendor `Data.lua` 内的书写顺序：
    /// 4 个具名档在前、8 个 Variable 档在后）。
    pub tree_versions: BTreeMap<String, Vec<JewelRadiusBandDef>>,
}

/// 一个环形档位（对应 PoB2 `data.jewelRadii` 的一行）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JewelRadiusBandDef {
    /// 档位标签：`Small` / `Medium` / `Large` / `Very Large` / `Variable`。
    pub label: String,
    /// 环形内半径基础值（tree units，未乘缩放因子）；具名档为 0（实心圆）。
    /// vendor-only 字段（pobr-tree first pass 无 inner 语义），源 `Modules/Data.lua:602-609`。
    pub inner: u32,
    /// 环形外半径基础值（tree units，未乘缩放因子）。
    /// 具名档与 pobr-tree `JEWEL_RADIUS_*`（= outer × 1.2）逐值一致。
    pub outer: u32,
    /// UI 高亮颜色码（PoB2 `col` 字段，`^xRRGGBB` 格式）。vendor-only 展示字段。
    pub colour: String,
}
