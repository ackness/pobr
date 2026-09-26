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
