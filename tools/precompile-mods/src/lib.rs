//! `precompile-mods` library surface, shared by `main.rs` and the integration tests.
//!
//! See the `main.rs` module doc for what the tool does. Module breakdown:
//! - [`corpus`]: collects the four corpus layers;
//! - [`canonical`]: byte-stable canonical form for a `Modifier`;
//! - [`parsed`]: per-line precompile → `parsed_mods.json` + coverage stats;
//! - [`report`]: coverage report → `parse-coverage.json`;
//! - [`check`]: `--check` overlay JSON validity check (contributor gate).

pub mod canonical;
pub mod check;
pub mod corpus;
pub mod parsed;
pub mod report;
