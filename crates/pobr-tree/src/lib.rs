//! pobr-tree: passive tree topology, allocated-node mod collection, jewel
//! socket gating, and radius jewels.
//!
//! Data flow: passive tree JSON (an array of
//! [`PassiveNodeDef`](pobr_data::catalog::PassiveNodeDef)) → [`PassiveTree`] →
//! combined with the allocated nodes in
//! [`PassiveTreeSpec`](pobr_data::passive_tree::PassiveTreeSpec) →
//! [`AllocatedNodeMods`] (modifier text plus
//! [`SourceKind::PassiveNode`](pobr_data::source::SourceKind::PassiveNode)
//! attribution, handed off to `pobr-core::passive` / `mod_parser` for
//! parsing). Radius jewels filter affected nodes by Euclidean distance via
//! [`compute_radius_jewel_effect_with_radii`] (band radii resolved from the
//! injected `base/jewel_radii.json` data; without injected data,
//! [`compute_radius_jewel_effect`] falls back to hardcoded constants that
//! match value-for-value).
//!
//! This crate depends only on `pobr-data`, does no I/O (the caller reads the
//! JSON string), and its queries are deterministic and immutable. Node data
//! uses the REAL authoritative type [`PassiveNodeDef`] (indexed by numeric
//! `skill` id); coordinates are injected by the caller via
//! [`PassiveTree::with_positions`] (the catalog doesn't carry coordinates —
//! see the module docs).

pub mod error;
pub mod node;
pub mod radius_jewel;
pub mod tree;

pub use error::TreeError;
pub use node::{
    AllocatedNodeMods, ClassContext, collect_allocated_mods, collect_allocated_mods_for_class,
};
pub use radius_jewel::{
    JEWEL_RADIUS_LARGE, JEWEL_RADIUS_MEDIUM, JEWEL_RADIUS_SMALL, JEWEL_RADIUS_VERY_LARGE,
    JewelRadius, PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER, RadiusJewelEffect,
    compute_radius_jewel_effect, compute_radius_jewel_effect_with_radii,
};
pub use tree::PassiveTree;
