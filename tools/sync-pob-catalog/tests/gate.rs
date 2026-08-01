//! Catalog diff and parity gates.
//!
//! An aggregated binary: previously-separate test files are now submodules
//! (7 -> 2), reducing the number of linked binaries to speed up builds.
#![allow(clippy::all)]

#[path = "gate/catalog_diff.rs"]
mod catalog_diff;
#[path = "gate/parity_gate.rs"]
mod parity_gate;
