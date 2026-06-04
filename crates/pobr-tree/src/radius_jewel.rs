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

/// Radius jewel 的半径常量（单位：tree units）。
///
/// 注意：这些数值为**占位常量**，可能与真实 PoE2 天赋树坐标系/半径不符。
/// 接入真实树坐标后需按一手数据校准（见 blocked_by_missing_data）。
pub const JEWEL_RADIUS_SMALL: f64 = 800.0;
pub const JEWEL_RADIUS_MEDIUM: f64 = 1200.0;
pub const JEWEL_RADIUS_LARGE: f64 = 1500.0;

/// 珠宝半径档位。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum JewelRadius {
    Small,
    Medium,
    Large,
    /// 自定义半径（tree units）。
    Custom(f64),
}

impl JewelRadius {
    /// 转换为以 tree units 计的半径数值。
    pub fn units(self) -> f64 {
        match self {
            JewelRadius::Small => JEWEL_RADIUS_SMALL,
            JewelRadius::Medium => JEWEL_RADIUS_MEDIUM,
            JewelRadius::Large => JEWEL_RADIUS_LARGE,
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
/// 以 `socket` 节点的坐标为圆心，按欧氏距离筛出落在 `radius` 内的**其它**节点
/// （socket 自身始终排除）。`positions` 提供 `skill id -> (x, y)`；缺坐标的候选节点不计入。
/// 结果按 `skill` id 升序排序以确保确定性。
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
