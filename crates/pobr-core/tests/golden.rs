//! PoB2 golden comparison and the display-field catalog.
//!
//! Aggregated binary: previously separate test files were merged into submodules to
//! cut the number of linked test binaries (53→8) and speed up builds. Each submodule
//! maps to `tests/golden/<name>.rs`; test cases and assertions are preserved one-for-one.
#![allow(clippy::all)]

#[path = "support/parse.rs"]
mod support;

#[path = "golden/display_catalog.rs"]
mod display_catalog;
#[path = "golden/pob2_golden.rs"]
mod pob2_golden;
