//! Calculation orchestration: session / env / perform fill / passes / attribution / character base.
//!
//! Aggregated binary: these were previously standalone test files, now merged into submodules
//! to cut the number of linked test binaries (53→8) and speed up builds.
//! Each submodule maps to `tests/engine/<name>.rs`; all test cases and assertions are preserved as-is.
#![allow(clippy::all)]

#[path = "support/parse.rs"]
mod support;

#[path = "engine/attribution.rs"]
mod attribution;
#[path = "engine/attribution_passes.rs"]
mod attribution_passes;
#[path = "engine/calc_env.rs"]
mod calc_env;
#[path = "engine/calc_minimal.rs"]
mod calc_minimal;
#[path = "engine/calc_modules.rs"]
mod calc_modules;
#[path = "engine/calc_session.rs"]
mod calc_session;
#[path = "engine/campaign.rs"]
mod campaign;
#[path = "engine/character_base.rs"]
mod character_base;
#[path = "engine/hand_pass.rs"]
mod hand_pass;
#[path = "engine/perform_fill.rs"]
mod perform_fill;
