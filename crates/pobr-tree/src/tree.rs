//! PassiveTree: the passive-tree topology container (node index), plus JSON
//! loading and derived queries.
//!
//! Nodes use the REAL authoritative type [`PassiveNodeDef`]
//! (`pobr_data::catalog`), indexed by numeric `skill` id. Coordinates (needed
//! for radius jewels / `nodes_in_radius`) aren't carried by the catalog, and
//! are injected by the caller via [`PassiveTree::with_positions`].

use std::collections::HashMap;

use pobr_data::prelude::*;

use crate::error::TreeError;
use crate::node::{AllocatedNodeMods, collect_allocated_mods};
use crate::radius_jewel::{
    JewelRadius, RadiusJewelEffect, compute_radius_jewel_effect,
    compute_radius_jewel_effect_with_radii,
};

/// The passive tree: node topology indexed by node `skill` id (`u32`).
///
/// Pure data plus read-only queries; holds no Build state (allocated nodes
/// are passed in via [`PassiveTreeSpec`]). `positions` is an optional
/// coordinate table (the catalog doesn't provide coordinates — see the
/// module docs).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PassiveTree {
    /// `skill id -> node definition`.
    pub nodes: HashMap<u32, PassiveNodeDef>,
    /// `skill id -> (x, y)` (tree units), used for radius queries; empty if
    /// the catalog has no coordinates.
    pub positions: HashMap<u32, (f64, f64)>,
}

impl PassiveTree {
    /// Builds a passive tree from JSON (an array of [`PassiveNodeDef`]), without coordinates.
    pub fn from_json(json: &str) -> Result<Self, TreeError> {
        let list: Vec<PassiveNodeDef> =
            serde_json::from_str(json).map_err(|e| TreeError::Json(e.to_string()))?;
        Ok(Self::from_nodes(list))
    }

    /// Builds from a node list (useful for programmatic construction without JSON).
    pub fn from_nodes(list: Vec<PassiveNodeDef>) -> Self {
        let nodes = list.into_iter().map(|n| (n.skill, n)).collect();
        Self {
            nodes,
            positions: HashMap::new(),
        }
    }

    /// Injects a coordinate table (`skill id -> (x, y)`), returning a new
    /// tree with positions set (immutable style).
    pub fn with_positions(mut self, positions: HashMap<u32, (f64, f64)>) -> Self {
        self.positions = positions;
        self
    }

    /// Looks up a node by [`NodeId`].
    pub fn node(&self, id: NodeId) -> Option<&PassiveNodeDef> {
        self.nodes.get(&id.0)
    }

    /// Looks up a node by its numeric `skill` id.
    pub fn node_by_skill(&self, skill: u32) -> Option<&PassiveNodeDef> {
        self.nodes.get(&skill)
    }

    /// Total node count.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the tree has no nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Returns the other nodes within `radius_units` (tree units) of
    /// `center`, sorted by skill id.
    ///
    /// Relies on already-injected `positions`: `center` or a candidate node
    /// missing coordinates is excluded; if `center` has no coordinates or the
    /// radius is invalid, returns an empty set (no error, to keep this a pure query).
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

    /// Collects the modifier text contributed by allocated nodes (see
    /// [`collect_allocated_mods`] for JewelSocket / Mastery gating).
    pub fn compute_node_mods(&self, spec: &PassiveTreeSpec) -> Vec<AllocatedNodeMods> {
        collect_allocated_mods(spec, &self.nodes)
    }

    /// Computes the effect range of a radius jewel socketed at `socket`
    /// (relies on already-injected `positions`).
    ///
    /// **Fallback entry point** (band radii use the `Default` data, which is
    /// value-for-value equal to the JSON); see
    /// [`PassiveTree::radius_jewel_effect_with_radii`] for the
    /// data-injection path.
    pub fn radius_jewel_effect(
        &self,
        socket: NodeId,
        radius: JewelRadius,
        jewel_mod_texts: Vec<String>,
    ) -> Result<RadiusJewelEffect, TreeError> {
        compute_radius_jewel_effect(socket.0, radius, &self.positions, jewel_mod_texts)
    }

    /// Computes the effect range of a radius jewel socketed at `socket`
    /// (data-injection version).
    ///
    /// Band effective radii are resolved from the injected `radii`
    /// (`base/jewel_radii.json`); see [`compute_radius_jewel_effect_with_radii`].
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
