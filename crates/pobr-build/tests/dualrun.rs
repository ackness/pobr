//! Old/new dual-run harness (statmap / config) for the migration period — delete this whole group once the migration cutover lands.
//!
//! Aggregated test binary: originally separate test files, merged into submodules (22→4) to cut the number of linked test binaries and speed up builds.
#![allow(clippy::all)]

#[path = "dualrun/config_dualrun.rs"]
mod config_dualrun;
#[path = "dualrun/statmap_dual_run.rs"]
mod statmap_dual_run;
