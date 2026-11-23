//! Stat-map observation regression suite.
//!
//! Aggregated test binary: originally separate test files, merged into submodules (22→4) to cut the number of linked test binaries and speed up builds.
#![allow(clippy::all)]

#[path = "dualrun/statmap_dual_run.rs"]
mod statmap_dual_run;
