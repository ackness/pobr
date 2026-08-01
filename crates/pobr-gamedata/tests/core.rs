//! Data loading: character constants / game constants / config options /
//! stat set storage.
//!
//! An aggregated binary: formerly-separate test files merged into
//! submodules (26→4), reducing the number of linked binaries to speed up builds.
#![allow(clippy::all)]

#[path = "core/full_stat_ingestion.rs"]
mod full_stat_ingestion;
#[path = "core/load_character_constants.rs"]
mod load_character_constants;
#[path = "core/load_config_options.rs"]
mod load_config_options;
#[path = "core/load_game_constants.rs"]
mod load_game_constants;
#[path = "core/multi_stat_sets.rs"]
mod multi_stat_sets;
#[path = "core/version_and_patch.rs"]
mod version_and_patch;
