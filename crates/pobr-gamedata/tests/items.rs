//! Data loading: base items / weapon types / unarmed / jewel radii / passive tree.
//!
//! An aggregated binary: formerly-separate test files merged into
//! submodules (26→4), reducing the number of linked binaries to speed up builds.
#![allow(clippy::all)]

#[path = "items/load_base_items.rs"]
mod load_base_items;
#[path = "items/load_jewel_radii.rs"]
mod load_jewel_radii;
#[path = "items/load_passive_tree.rs"]
mod load_passive_tree;
#[path = "items/load_unarmed_data.rs"]
mod load_unarmed_data;
#[path = "items/load_weapon_types.rs"]
mod load_weapon_types;
