//! Parsing layer: modifier text -> Mod (mod_parser / special / modcache golden).
//!
//! Aggregated binary: previously-independent test files merged into submodules to
//! cut the number of linked test binaries (53 -> 8) and speed up builds.
//! Each submodule is `tests/parser/<name>.rs`; all test cases and assertions are
//! preserved as-is.
#![allow(clippy::all)]

#[path = "support/parse.rs"]
mod support;

#[path = "parser/mod_parser.rs"]
mod mod_parser;
#[path = "parser/parser_modcache_golden.rs"]
mod parser_modcache_golden;
#[path = "parser/special_mods_gate.rs"]
mod special_mods_gate;
