//! Data loading: mods / local mods / special / high-precision / base player
//! mods / overlays / curse priority.
//!
//! An aggregated binary: formerly-separate test files merged into
//! submodules (26→4), reducing the number of linked binaries to speed up builds.
#![allow(clippy::all)]

#[path = "mods/load_base_player_mods.rs"]
mod load_base_player_mods;
#[path = "mods/load_curse_priority.rs"]
mod load_curse_priority;
#[path = "mods/load_high_precision_mods.rs"]
mod load_high_precision_mods;
#[path = "mods/load_item_overlay.rs"]
mod load_item_overlay;
#[path = "mods/load_local_mods.rs"]
mod load_local_mods;
#[path = "mods/load_mods.rs"]
mod load_mods;
#[path = "mods/load_special_mods.rs"]
mod load_special_mods;
