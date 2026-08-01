//! Ailments: magnitude and application (ignite / chill / shock / poison / bleed).
//!
//! Aggregated binary: previously-independent test files merged into submodules to
//! cut the number of linked test binaries (53 -> 8) and speed up builds.
//! Each submodule is `tests/ailments/<name>.rs`; all test cases and assertions are
//! preserved as-is.
#![allow(clippy::all)]

#[path = "ailments/ailment.rs"]
mod ailment;
#[path = "ailments/ailment_apply.rs"]
mod ailment_apply;
