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
    /// Resolves the effective radius (tree units) from injected radius-jewel
    /// band data.
    ///
    /// Named bands (Small/Medium/Large/VeryLarge) are looked up by label in
    /// `radii`; effective radius = `outer × distance_multiplier` (mirroring
    /// PoB2's `outerSquared` semantics in `setJewelRadiiGlobally`). `Custom`
    /// returns the caller-provided value directly (scaling factor already applied).
    ///
    /// Selects the newest numeric version, never lexicographic ordering.
    pub fn units_with_radii(self, radii: &JewelRadiiDef) -> f64 {
        self.units_for_tree(radii, None)
    }

    /// Resolves the newest radius table no newer than the selected tree.
    pub fn units_for_tree(self, radii: &JewelRadiiDef, tree_version: Option<&str>) -> f64 {
        fn version(value: &str) -> Option<Vec<u32>> {
            value
                .split('_')
                .map(str::parse)
                .collect::<Result<_, _>>()
                .ok()
        }
        let target = tree_version.and_then(version);
        let label = match self {
            JewelRadius::Small => "Small",
            JewelRadius::Medium => "Medium",
            JewelRadius::Large => "Large",
            JewelRadius::VeryLarge => "Very Large",
            JewelRadius::Custom(r) => return r,
        };
        radii
            .tree_versions
            .iter()
            .filter_map(|(key, bands)| version(key).map(|v| (v, bands)))
            .filter(|(v, _)| target.as_ref().is_none_or(|target| v <= target))
            .max_by(|(a, _), (b, _)| a.cmp(b))
            .and_then(|(_, bands)| bands.iter().find(|band| band.label == label))
            .map(|band| f64::from(band.outer) * radii.distance_multiplier)
            .unwrap_or_else(|| {
                // A table without this band: the built-in table
                // (`JewelRadiiDef::default`, equal to `base/jewel_radii.json`).
                let fallback = JewelRadiiDef::default();
                fallback
                    .tree_versions
                    .values()
                    .flatten()
                    .find(|band| band.label == label)
                    .map_or(0.0, |band| {
                        f64::from(band.outer) * fallback.distance_multiplier
                    })
            })
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
