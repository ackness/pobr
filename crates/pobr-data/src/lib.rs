/// PoE2 data version shipped with the repo — the CDN patch number, and the name
/// of the `data/<DATA_VERSION>/` directory. This is the compile-time default;
/// [`data_version`] applies the runtime override on top.
///
/// Shipping a different version means editing this constant and nothing else.
/// To point somewhere else temporarily, set `POBR_DATA_VERSION` or write a
/// `data/CURRENT` marker file rather than touching code — see [`data_version`]
/// and `pobr_gamedata::current_data_dir`.
///
/// Kept in sync with `data/CURRENT`. Golden and parity tests deliberately do
/// not read this constant, so it can move ahead to newer data without turning
/// them red; see [`GOLDEN_PARITY_DATA_VERSION`]. That the newer data still runs
/// at all is covered by the `multi_version` smoke test.
pub const DATA_VERSION: &str = "4.5.4.8";

/// Data version the checked-in golden and parity numbers were recorded against.
///
/// Those numbers are version-specific — the PoB2 `player_stats` in
/// `examples/demo-bd-test/*/meta.json`, per-domain row counts, vendor-commit
/// spot checks — so the tests load the version pinned here instead of the
/// active one. Bump this when the goldens are re-recorded. Whether the engine
/// works across versions at all is a separate question, answered by the
/// `multi_version` smoke test: it runs a calc against every `data/<ver>/` and
/// asserts the results are dimensionally sane without comparing to goldens.
pub const GOLDEN_PARITY_DATA_VERSION: &str = "4.5.4.8";

/// Data version to use at runtime: `POBR_DATA_VERSION` when set, otherwise the
/// [`DATA_VERSION`] constant.
///
/// This crate does no file I/O, so reading the process environment is as far as
/// discovery goes here. To also honour the `data/CURRENT` marker file, call
/// `pobr_gamedata::data_version` instead. Exporting `POBR_DATA_VERSION=<version>`
/// after dropping in new data repoints every caller, tests included, with no
/// code change.
pub fn data_version() -> String {
    std::env::var("POBR_DATA_VERSION").unwrap_or_else(|_| DATA_VERSION.to_string())
}

pub mod build_config;
pub mod catalog;
pub mod constants;
pub mod damage;
pub mod display_stat;
pub mod game_data;
pub mod gem;
pub mod item;
pub mod minion;
pub mod modifier;
pub mod monster;
pub mod passive_tree;
pub mod skill;
mod skill_type_names;
pub mod source;
pub mod stat;

pub mod prelude {
    pub use crate::build_config::*;
    pub use crate::catalog::*;
    pub use crate::constants::*;
    pub use crate::damage::*;
    pub use crate::display_stat::*;
    pub use crate::game_data::*;
    pub use crate::gem::*;
    pub use crate::item::*;
    pub use crate::minion::*;
    pub use crate::modifier::*;
    pub use crate::monster::*;
    pub use crate::passive_tree::*;
    pub use crate::skill::*;
    pub use crate::source::*;
    pub use crate::stat::*;
}
