//! Extraction logic: vendor Lua extraction / parser rules / quality / special_mods / scan.
//!
//! An aggregated binary: previously-separate test files are now submodules
//! (7 -> 2), reducing the number of linked binaries to speed up builds.
#![allow(clippy::all)]

#[path = "extract/extract_lua.rs"]
mod extract_lua;
#[path = "extract/extract_parser_rules.rs"]
mod extract_parser_rules;
#[path = "extract/extract_quality.rs"]
mod extract_quality;
#[path = "extract/scan.rs"]
mod scan;
#[path = "extract/special_mods_patterns.rs"]
mod special_mods_patterns;
