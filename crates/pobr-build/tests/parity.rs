//! PoB2 comparison / regression test aggregate binary.
//!
//! Two kinds of contract (see `devs/docs/architecture/16-data-versioning-and-iteration.md` §5):
//!   A) **Version-pinned golden reference** — asserts "externally recorded exact
//!      values" (PoB2 `player_stats` / oracle / self-snapshot); data loads via
//!      `pobr_data::GOLDEN_PARITY_DATA_VERSION` (decoupled from the active
//!      `DATA_VERSION`). It's a reference snapshot for one PoB2 version, not a
//!      version-independent logic gate; moving to a new version requires
//!      **re-recording the golden values**, not tweaking data/tests to fit.
//!   B) **Version-independent logic / smoke** — only asserts relations, ranges,
//!      determinism, and import correctness; no recorded values; runs against the
//!      active `data_version()` (or every ingested version).
//!
//! Aggregated test binary: originally separate test files, merged into submodules
//! (22→4) to cut the number of linked test binaries and speed up builds.
//! Physical directories are no longer grouped by category — `#[path]` already
//! decouples file location from the binary, and several files use file-relative
//! `include_str!`, so moving them would be fragile. Use the grouping below plus
//! each file's own loading convention (A pins GOLDEN_PARITY, B uses
//! `data_version()` or loads no data) as the source of truth.
#![allow(clippy::all)]

// A) Version-pinned golden reference (pins pobr_data::GOLDEN_PARITY_DATA_VERSION)
#[path = "parity/coc_trigger_golden.rs"]
mod coc_trigger_golden;
#[path = "parity/crossbow_reload_golden.rs"]
mod crossbow_reload_golden;
#[path = "parity/defence_panels_golden.rs"]
mod defence_panels_golden;
#[path = "parity/golden_canary.rs"]
// extra cross damage/defence canary calibration anchor (not a gate replacement)
mod golden_canary;
#[path = "parity/ninja_parity.rs"]
mod ninja_parity;
#[path = "parity/pob2_parity.rs"]
mod pob2_parity;
#[path = "parity/skill_dot_golden.rs"]
mod skill_dot_golden;
#[path = "parity/stored_hand_output.rs"]
mod stored_hand_output;

// B) Version-independent logic / smoke / self-snapshot regression (active version / all versions)
#[path = "parity/e2e_real_build.rs"]
// real build end-to-end: only asserts ranges/determinism/import correctness
mod e2e_real_build;
#[path = "parity/golden_regression.rs"]
// uses calculate(), zero data loading -> pure calc-code regression lock
mod golden_regression;
#[path = "parity/multi_version.rs"]
// iterates every ingested version, proving calc is version-independent
mod multi_version;
#[path = "parity/special_oracle_differential.rs"]
// active special_mods vs live oracle (skip-guarded)
mod special_oracle_differential;
// gap B: per-build treeVersion capture + mismatch diagnosis
#[path = "parity/tree_version_diag.rs"]
mod tree_version_diag;
