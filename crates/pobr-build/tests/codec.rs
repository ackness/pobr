//! Build Code codec + config fixtures.
//!
//! Aggregated test binary: originally separate test files, merged into submodules (22→4) to cut the number of linked test binaries and speed up builds.
#![allow(clippy::all)]

#[path = "codec/build_code.rs"]
mod build_code;
#[path = "codec/config_fixtures.rs"]
mod config_fixtures;
