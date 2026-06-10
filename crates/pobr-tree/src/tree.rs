//! PassiveTree：天赋树拓扑容器（节点索引）+ JSON 加载 + 派生查询。
//!
//! 节点采用 REAL 权威类型 [`PassiveNodeDef`]（`pobr_data::catalog`），以数值 `skill`
//! id 索引。坐标（radius jewel / `nodes_in_radius` 所需）catalog 不携带，由调用方经
//! [`PassiveTree::with_positions`] 注入。

use std::collections::HashMap;

use pobr_data::prelude::*;

use crate::error::TreeError;
use crate::node::{AllocatedNodeMods, collect_allocated_mods};
use crate::radius_jewel::{
    JewelRadius, RadiusJewelEffect, compute_radius_jewel_effect,
    compute_radius_jewel_effect_with_radii,
};

/// 天赋树：按节点 `skill` id（`u32`）索引的节点拓扑。
///
/// 纯数据 + 只读查询，不持有任何 Build 状态（allocated 节点由 [`PassiveTreeSpec`] 传入）。
/// `positions` 为可选坐标表（catalog 不提供坐标，见模块文档）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PassiveTree {
    /// `skill id -> 节点定义`。
    pub nodes: HashMap<u32, PassiveNodeDef>,
    /// `skill id -> (x, y)`（tree units），radius 查询用；catalog 无坐标时为空。
    pub positions: HashMap<u32, (f64, f64)>,
}

impl PassiveTree {
    /// 从 JSON（[`PassiveNodeDef`] 数组）构建天赋树（不含坐标）。
    pub fn from_json(json: &str) -> Result<Self, TreeError> {
        let list: Vec<PassiveNodeDef> =
            serde_json::from_str(json).map_err(|e| TreeError::Json(e.to_string()))?;
        Ok(Self::from_nodes(list))
    }

    /// 从节点列表构建（便于程序化构造，无需 JSON）。
    pub fn from_nodes(list: Vec<PassiveNodeDef>) -> Self {
        let nodes = list.into_iter().map(|n| (n.skill, n)).collect();
        Self {
            nodes,
            positions: HashMap::new(),
        }
    }

    /// 注入坐标表（`skill id -> (x, y)`），返回带坐标的新树（不可变风格）。
    pub fn with_positions(mut self, positions: HashMap<u32, (f64, f64)>) -> Self {
        self.positions = positions;
        self
    }

    /// 按 [`NodeId`] 查找节点。
    pub fn node(&self, id: NodeId) -> Option<&PassiveNodeDef> {
        self.nodes.get(&id.0)
    }

    /// 按数值 `skill` id 查找节点。
    pub fn node_by_skill(&self, skill: u32) -> Option<&PassiveNodeDef> {
        self.nodes.get(&skill)
    }

    /// 节点总数。
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// 是否为空树。
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// 以 `center` 节点为圆心，返回半径（tree units）内的其它节点（按 skill id 排序）。
    ///
    /// 依赖已注入的 `positions`：`center` 或候选节点缺坐标则不计入；
    /// `center` 缺坐标或半径非法时返回空集合（不报错，便于纯查询场景）。
    pub fn nodes_in_radius(&self, center: NodeId, radius_units: f64) -> Vec<NodeId> {
        if !radius_units.is_finite() || radius_units < 0.0 {
            return Vec::new();
        }
        let Some(&(cx, cy)) = self.positions.get(&center.0) else {
            return Vec::new();
        };
        let radius_sq = radius_units * radius_units;
        let mut out: Vec<u32> = self
            .positions
            .iter()
            .filter(|(id, _)| **id != center.0)
            .filter(|(_, (x, y))| {
                let dx = x - cx;
                let dy = y - cy;
                dx * dx + dy * dy <= radius_sq
            })
            .map(|(id, _)| *id)
            .collect();
        out.sort_unstable();
        out.into_iter().map(NodeId).collect()
    }

    /// 收集已分配节点贡献的 modifier 文本（JewelSocket / Mastery gating 见 [`collect_allocated_mods`]）。
    pub fn compute_node_mods(&self, spec: &PassiveTreeSpec) -> Vec<AllocatedNodeMods> {
        collect_allocated_mods(spec, &self.nodes)
    }

    /// 计算挂在 `socket` 上的 radius jewel 影响范围（依赖已注入的 `positions`）。
    ///
    /// **fallback 入口**（档位半径走 `Default` 数据，与 JSON 逐值相等）；
    /// 数据注入路径见 [`PassiveTree::radius_jewel_effect_with_radii`]。
    pub fn radius_jewel_effect(
        &self,
        socket: NodeId,
        radius: JewelRadius,
        jewel_mod_texts: Vec<String>,
    ) -> Result<RadiusJewelEffect, TreeError> {
        compute_radius_jewel_effect(socket.0, radius, &self.positions, jewel_mod_texts)
    }

    /// 计算挂在 `socket` 上的 radius jewel 影响范围（数据注入版）。
    ///
    /// 档位有效半径由注入的 `radii`（`base/jewel_radii.json`）解析，
    /// 见 [`compute_radius_jewel_effect_with_radii`]。
    pub fn radius_jewel_effect_with_radii(
        &self,
        socket: NodeId,
        radius: JewelRadius,
        radii: &pobr_data::catalog::jewel_radii::JewelRadiiDef,
        jewel_mod_texts: Vec<String>,
    ) -> Result<RadiusJewelEffect, TreeError> {
        compute_radius_jewel_effect_with_radii(
            socket.0,
            radius,
            radii,
            &self.positions,
            jewel_mod_texts,
        )
    }
}
