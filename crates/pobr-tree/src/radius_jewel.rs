//! Radius jewels: centered on a socket node, filter nodes within its effect
//! radius by Euclidean distance.
//!
//! The REAL authoritative node data [`PassiveNodeDef`] carries no planar
//! coordinates (GGG's PoE2 export leaves positions embedded in the
//! orbit/group layout instead of giving standalone `x`/`y` fields — see the
//! catalog docs). Radius calculations therefore rely on an **externally
//! provided position table** `positions` (`skill id -> (x, y)`, in tree
//! units), injected into `PassiveTree` via
//! [`PassiveTree::with_positions`](crate::PassiveTree::with_positions). Nodes
//! missing coordinates are treated as outside every radius (if the socket
//! itself lacks coordinates, [`TreeError::NodePositionMissing`] is returned).

use std::collections::HashMap;

use pobr_data::catalog::jewel_radii::JewelRadiiDef;
use serde::{Deserialize, Serialize};

use crate::error::TreeError;

/// PoE2's passive-tree jewel distance scaling factor.
///
/// Source: PoB2 `Data/Misc.lua` `data.gameConstants["PassiveTreeJewelDistanceMultiplier"]`,
/// transcribed from `GameConstants.dat`. An `outer` value is multiplied by
/// this factor before comparing against node Euclidean distance, equivalent
/// to `outerSquared = outer * outer * 1.2 * 1.2` (PoB2 Data.lua
/// setJewelRadiiGlobally).
///
/// **Deprecation note**: this constant and `JEWEL_RADIUS_*` are now
/// **fallback-only** — the live calculation path consumes the injected
/// [`JewelRadiiDef`] (`base/jewel_radii.json`, which is value-for-value equal
/// to this constant group, pinned by tests) via
/// [`compute_radius_jewel_effect_with_radii`]. **Do not add new consumers of
/// the calculation path here**; these are kept for the no-data fallback path
/// ([`JewelRadius::units`]) and as anchors for test expectations.
pub const PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER: f64 = 1.2;

/// Base outer radius per radius-jewel band (tree units, before the scaling factor).
///
/// Source: PoB2 `src/Modules/Data.lua` `data.jewelRadii["0_1"]` (PoE2's first
/// 0.x release). The actual comparison threshold is
/// `outer * PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER` (see [`JewelRadius::units`]).
///
/// | Band       | outer | Effective radius (×1.2) |
/// |------------|-------|--------------------------|
/// | Small      | 1000  | 1200.0                   |
/// | Medium     | 1150  | 1380.0                   |
/// | Large      | 1300  | 1560.0                   |
/// | Very Large | 1500  | 1800.0                   |
///
/// **Deprecation note**: fallback-only, do not add new consumers of the
/// calculation path (see the note on
/// [`PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER`]).
pub const JEWEL_RADIUS_SMALL: f64 = 1000.0 * PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER;
pub const JEWEL_RADIUS_MEDIUM: f64 = 1150.0 * PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER;
pub const JEWEL_RADIUS_LARGE: f64 = 1300.0 * PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER;
pub const JEWEL_RADIUS_VERY_LARGE: f64 = 1500.0 * PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER;

/// A jewel radius band.
///
/// The named bands match the `label` field in PoB2's
/// `data.jewelRadii["0_1"]`; `Custom` lets the caller supply an effective
/// radius directly (tree units, scaling factor already applied), for the
/// outer bound of Variable-radius jewels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum JewelRadius {
    /// outer=1000, effective radius 1200 tree units (×1.2).
    Small,
    /// outer=1150, effective radius 1380 tree units (×1.2).
    Medium,
    /// outer=1300, effective radius 1560 tree units (×1.2).
    Large,
    /// outer=1500, effective radius 1800 tree units (×1.2).
    VeryLarge,
    /// A custom effective radius (tree units, scaling factor already
    /// applied by the caller). Used for Variable-radius jewels and other
    /// non-standard bands.
    Custom(f64),
}

impl JewelRadius {
    /// Returns the effective radius (tree units), i.e.
    /// `outer * PassiveTreeJewelDistanceMultiplier`.
    ///
    /// **Fallback path** (used when no data is injected; the hardcoded
    /// constants match `base/jewel_radii.json` value-for-value). See
    /// [`JewelRadius::units_with_radii`] for the data-injection path.
    ///
    /// Mirrors PoB2's `Data.lua` calculation:
    /// ```text
    /// outerSquared = outer * outer * PassiveTreeJewelDistanceMultiplier^2
    /// ```
    /// equivalent to computing `effective = outer * 1.2` first, then squaring
    /// it as the Euclidean-distance-squared threshold.
    pub fn units(self) -> f64 {
        match self {
            JewelRadius::Small => JEWEL_RADIUS_SMALL,
            JewelRadius::Medium => JEWEL_RADIUS_MEDIUM,
            JewelRadius::Large => JEWEL_RADIUS_LARGE,
            JewelRadius::VeryLarge => JEWEL_RADIUS_VERY_LARGE,
            JewelRadius::Custom(r) => r,
        }
    }

    /// Resolves the effective radius (tree units) from injected radius-jewel
    /// band data.
    ///
    /// Named bands (Small/Medium/Large/VeryLarge) are looked up by label in
    /// `radii`; effective radius = `outer × distance_multiplier` (mirroring
    /// PoB2's `outerSquared` semantics in `setJewelRadiiGlobally`). `Custom`
    /// returns the caller-provided value directly (scaling factor already applied).
    ///
    /// Tree version selection: PoB2 picks the newest version group `<=` the
    /// target tree version — currently there's only the `0_1` group, so
    /// taking the max key of `tree_versions` (the last `BTreeMap` entry) is
    /// equivalent. If the data is missing the requested named band
    /// (malformed/truncated data), this falls back to the hardcoded
    /// constants (value-for-value equal to the default data, so behaviour is unchanged).
    pub fn units_with_radii(self, radii: &JewelRadiiDef) -> f64 {
        let label = match self {
            JewelRadius::Small => "Small",
            JewelRadius::Medium => "Medium",
            JewelRadius::Large => "Large",
            JewelRadius::VeryLarge => "Very Large",
            JewelRadius::Custom(r) => return r,
        };
        radii
            .tree_versions
            .values()
            .next_back()
            .and_then(|bands| bands.iter().find(|band| band.label == label))
            .map(|band| f64::from(band.outer) * radii.distance_multiplier)
            .unwrap_or_else(|| self.units())
    }
}

/// The result of a radius jewel calculation: the affected node set plus the
/// jewel's own modifier text.
///
/// `socket` / `affected_nodes` are represented as node `skill` ids (`u32`).
/// The REAL [`NodeId`](pobr_data::passive_tree::NodeId) doesn't derive
/// `Serialize`/`Ord`, so this persists and sorts by the stable numeric id
/// instead; the caller can wrap it back into `NodeId` as needed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RadiusJewelEffect {
    pub socket: u32,
    /// The `skill` ids of affected nodes, ascending numeric order (deterministic).
    pub affected_nodes: Vec<u32>,
    pub mod_texts: Vec<String>,
}

/// Computes a radius jewel's effect range (**fallback entry point**: the
/// no-injected-data path).
///
/// Equivalent to `compute_radius_jewel_effect_with_radii(.., &JewelRadiiDef::default(), ..)`
/// — the `Default` fallback is value-for-value equal to `base/jewel_radii.json`
/// (an invariant carried over from the migration), so both paths produce the
/// same output. See [`compute_radius_jewel_effect_with_radii`] for the
/// data-injection path.
pub fn compute_radius_jewel_effect(
    socket: u32,
    radius: JewelRadius,
    positions: &HashMap<u32, (f64, f64)>,
    jewel_mod_texts: Vec<String>,
) -> Result<RadiusJewelEffect, TreeError> {
    compute_radius_jewel_effect_with_radii(
        socket,
        radius,
        &JewelRadiiDef::default(),
        positions,
        jewel_mod_texts,
    )
}

/// Computes a radius jewel's effect range (data-injection version; the main
/// entry point for the calculation path).
///
/// Centered on the `socket` node's coordinates, filters **other** nodes
/// within the effective radius by Euclidean distance (the socket itself is
/// always excluded). The effective radius is resolved from the injected
/// `radii` (`base/jewel_radii.json`) by band label (see
/// [`JewelRadius::units_with_radii`]); `positions` provides `skill id -> (x, y)`;
/// candidate nodes missing coordinates are excluded. Results are sorted
/// ascending by `skill` id for determinism.
///
/// Comparison formula: `dx² + dy² <= (outer × distance_multiplier)²`,
/// mirroring PoB2 Data.lua's
/// `radiusInfo.outerSquared = outer * outer * PassiveTreeJewelDistanceMultiplier²`.
///
/// Errors:
/// - `socket` missing coordinates → [`TreeError::NodePositionMissing`].
/// - A negative or non-finite radius (NaN/Inf) → [`TreeError::InvalidRadius`].
pub fn compute_radius_jewel_effect_with_radii(
    socket: u32,
    radius: JewelRadius,
    radii: &JewelRadiiDef,
    positions: &HashMap<u32, (f64, f64)>,
    jewel_mod_texts: Vec<String>,
) -> Result<RadiusJewelEffect, TreeError> {
    let radius_units = radius.units_with_radii(radii);
    if !radius_units.is_finite() || radius_units < 0.0 {
        return Err(TreeError::InvalidRadius(radius_units));
    }

    let center = *positions
        .get(&socket)
        .ok_or(TreeError::NodePositionMissing(socket))?;

    // Mirrors PoB2: outerSquared = outer * outer * multiplier * multiplier,
    // equivalent to (outer * multiplier)^2; radius_units here is already
    // outer * multiplier.
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

    // HashMap iteration order is unspecified; sort to keep output deterministic.
    affected.sort_unstable();

    Ok(RadiusJewelEffect {
        socket,
        affected_nodes: affected,
        mod_texts: jewel_mod_texts,
    })
}
