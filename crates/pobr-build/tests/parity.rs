//! PoB2 comparison / regression test aggregate binary.
//!
//! Two kinds of contract:
//!   A) **Recorded PoB2 reference** — asserts externally recorded exact values
//!      (PoB2 `player_stats` / oracle) against the active data
//!      version. L3 agreement gates follow the pinned reference's data version;
//!      historical fixtures keep their recorded identity. See the maintainer guide.
//!   B) **Logic / smoke and native L2 snapshot** — relations, ranges,
//!      determinism, and import correctness run against the
//!      active `data_version()` (or every ingested version). L2 separately
//!      records native PoBR outputs under the exact runtime lock identity.
//!
//! Aggregated test binary: originally separate test files, merged into submodules
//! (22→4) to cut the number of linked test binaries and speed up builds.
//! Physical directories are no longer grouped by category — `#[path]` already
//! decouples file location from the binary, and several files use file-relative
//! `include_str!`, so moving them would be fragile. Use the grouping below plus
//! each file's own loading convention (A and B both use `data_version()`,
//! or load no data) as the source of truth.
#![allow(clippy::all)]

// A) Recorded PoB2 reference (active data version)
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
#[path = "parity/reference_version.rs"]
mod reference_version;
#[path = "parity/skill_dot_golden.rs"]
mod skill_dot_golden;
#[path = "parity/stored_hand_output.rs"]
mod stored_hand_output;

// B) Version-independent logic / smoke / self-snapshot regression (active version / all versions)
#[path = "parity/diagnose_build.rs"]
// one build: import losses, reference differences, and every input the calculation dropped
mod diagnose_build;
#[path = "parity/e2e_real_build.rs"]
// real build end-to-end: only asserts ranges/determinism/import correctness
mod e2e_real_build;
#[path = "parity/golden_regression.rs"]
// minimal text-only calculation and codec regressions
mod golden_regression;
#[path = "parity/multi_version.rs"]
// iterates every ingested version, proving calc is version-independent
mod multi_version;
#[path = "parity/output_snapshot.rs"]
mod output_snapshot;
#[path = "parity/reference_report.rs"]
// import keeps every used item; dashboard against the stats a build code carries
mod reference_report;
#[path = "parity/special_oracle_differential.rs"]
// active special_mods vs live oracle (skip-guarded)
mod special_oracle_differential;
// gap B: per-build treeVersion capture + mismatch diagnosis
#[path = "parity/tree_version_diag.rs"]
mod tree_version_diag;
