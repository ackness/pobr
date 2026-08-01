//! Aggregation layer: ModDb storage / queries / attribution trace / enemy ModDb.
//!
//! An aggregated binary: previously-separate test files are merged into submodules
//! to cut the number of linked test binaries (53 -> 8) and speed up builds.
//! Each submodule is `tests/aggregation/<name>.rs`; test cases and assertions are kept as-is.
#![allow(clippy::all)]

#[path = "aggregation/enemy_mod_db.rs"]
mod enemy_mod_db;
#[path = "aggregation/mod_db.rs"]
mod mod_db;
#[path = "aggregation/mod_db_traced.rs"]
mod mod_db_traced;
#[path = "aggregation/trace.rs"]
mod trace;
