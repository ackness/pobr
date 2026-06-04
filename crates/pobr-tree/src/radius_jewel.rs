//! Radius jewel：以 socket 节点为圆心，按欧氏距离筛出影响范围内的节点。
//!
//! REAL 权威节点数据 [`PassiveNodeDef`] 不携带平面坐标（GGG PoE2 导出把坐标留在
//! orbit/group 布局里，未给独立的 `x`/`y`，见 catalog 文档）。因此范围计算依赖**外部
//! 提供的坐标表** `positions`（`skill id -> (x, y)`，tree units）；`PassiveTree` 通过
//! [`PassiveTree::with_positions`](crate::PassiveTree::with_positions) 注入。坐标缺失的
//! 节点被视为不在任何半径内（socket 自身缺坐标则报 [`TreeError::NodePositionMissing`]）。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::TreeError;

/// PoE2 天赋树珠宝距离缩放因子。
///
/// 来源：PoB2 `Data/Misc.lua` `data.gameConstants["PassiveTreeJewelDistanceMultiplier"]`，
/// 转录自 `GameConstants.dat`。`outer` 值乘以该系数后再与节点欧氏距离比较，等价于
/// `outerSquared = outer * outer * 1.2 * 1.2`（PoB2 Data.lua setJewelRadiiGlobally）。
pub const PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER: f64 = 1.2;

/// Radius jewel 各档 outer 基础半径（tree units，未乘缩放因子）。
///
/// 来源：PoB2 `src/Modules/Data.lua` `data.jewelRadii["0_1"]`（PoE2 0.x 首版）。
/// 实际比较阈值 = `outer * PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER`（见 [`JewelRadius::units`]）。
///
/// | 档位       | outer | 实际半径（×1.2）|
/// |------------|-------|----------------|
/// | Small      | 1000  | 1200.0          |
/// | Medium     | 1150  | 1380.0          |
/// | Large      | 1300  | 1560.0          |
/// | Very Large | 1500  | 1800.0          |
pub const JEWEL_RADIUS_SMALL: f64 = 1000.0 * PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER;
pub const JEWEL_RADIUS_MEDIUM: f64 = 1150.0 * PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER;
pub const JEWEL_RADIUS_LARGE: f64 = 1300.0 * PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER;
pub const JEWEL_RADIUS_VERY_LARGE: f64 = 1500.0 * PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER;

/// 珠宝半径档位。
///
/// 档位对应 PoB2 `data.jewelRadii["0_1"]` 中 `label` 字段；`Custom` 由调用方直接提供
/// 有效半径（tree units，已含缩放因子），适合 Variable 半径珠宝的 outer 边界。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum JewelRadius {
    /// outer=1000，实际半径 1200 tree units（×1.2）。
    Small,
    /// outer=1150，实际半径 1380 tree units（×1.2）。
    Medium,
    /// outer=1300，实际半径 1560 tree units（×1.2）。
    Large,
    /// outer=1500，实际半径 1800 tree units（×1.2）。
    VeryLarge,
    /// 自定义有效半径（tree units，调用方已乘缩放因子）。适用于 Variable 半径等非标准档位。
    Custom(f64),
}

impl JewelRadius {
    /// 返回有效半径（tree units），即 `outer * PassiveTreeJewelDistanceMultiplier`。
    ///
    /// 对标 PoB2 `Data.lua` 的计算：
    /// ```text
    /// outerSquared = outer * outer * PassiveTreeJewelDistanceMultiplier^2
    /// ```
    /// 等价于先算 `effective = outer * 1.2`，再取平方作为欧氏距离的平方阈值。
    pub fn units(self) -> f64 {
        match self {
            JewelRadius::Small => JEWEL_RADIUS_SMALL,
            JewelRadius::Medium => JEWEL_RADIUS_MEDIUM,
            JewelRadius::Large => JEWEL_RADIUS_LARGE,
            JewelRadius::VeryLarge => JEWEL_RADIUS_VERY_LARGE,
            JewelRadius::Custom(r) => r,
        }
    }
}

/// 一个 radius jewel 的计算结果：受影响节点集合 + 珠宝携带的 modifier 文本。
///
/// `socket` / `affected_nodes` 以节点 `skill` id（`u32`）表示。REAL [`NodeId`](pobr_data::passive_tree::NodeId)
/// 不派生 `Serialize`/`Ord`，这里以稳定的数值 id 持久化并排序，调用方可自行包回 `NodeId`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RadiusJewelEffect {
    pub socket: u32,
    /// 受影响节点的 `skill` id，按数值升序（确定性）。
    pub affected_nodes: Vec<u32>,
    pub mod_texts: Vec<String>,
}

/// 计算 radius jewel 影响范围。
///
/// 以 `socket` 节点的坐标为圆心，按欧氏距离筛出落在 `radius.units()` 内的**其它**节点
/// （socket 自身始终排除）。`positions` 提供 `skill id -> (x, y)`；缺坐标的候选节点不计入。
/// 结果按 `skill` id 升序排序以确保确定性。
///
/// 比较公式：`dx² + dy² <= (radius.units())²`，对标 PoB2 Data.lua 中
/// `radiusInfo.outerSquared = outer * outer * PassiveTreeJewelDistanceMultiplier²`。
///
/// 错误：
/// - `socket` 缺坐标 → [`TreeError::NodePositionMissing`]。
/// - 半径为负或非有限（NaN/Inf）→ [`TreeError::InvalidRadius`]。
pub fn compute_radius_jewel_effect(
    socket: u32,
    radius: JewelRadius,
    positions: &HashMap<u32, (f64, f64)>,
    jewel_mod_texts: Vec<String>,
) -> Result<RadiusJewelEffect, TreeError> {
    let radius_units = radius.units();
    if !radius_units.is_finite() || radius_units < 0.0 {
        return Err(TreeError::InvalidRadius(radius_units));
    }

    let center = *positions
        .get(&socket)
        .ok_or(TreeError::NodePositionMissing(socket))?;

    // 对标 PoB2：outerSquared = outer * outer * multiplier * multiplier
    // 等价于 (outer * multiplier)^2，此处 radius_units 已是 outer * multiplier。
    let radius_sq = radius_units * radius_units;

    let mut affected: Vec<u32> = positions
        .iter()
        .filter(|(id, _)| **id != socket)
        .filter(|(_, (x, y))| {
            let dx = x - center.0;
            let dy = y - center.1;
            dx * dx + dy * dy <= radius_sq
        })
        .map(|(id, _)| *id)
        .collect();

    // HashMap 迭代顺序不确定，排序以保证输出确定性。
    affected.sort_unstable();

    Ok(RadiusJewelEffect {
        socket,
        affected_nodes: affected,
        mod_texts: jewel_mod_texts,
    })
}
