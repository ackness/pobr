//! PoB2 catalog scanning and the curated special_mods pattern check.
//!
//! An aggregated binary: separate test files are submodules, reducing the
//! number of linked binaries to speed up builds.
#![allow(clippy::all)]

#[path = "catalog/scan.rs"]
mod scan;
#[path = "catalog/special_mods_patterns.rs"]
mod special_mods_patterns;
