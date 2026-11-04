//! PoBR's data contract: the serialized types a data package contains and the
//! loader (`pobr-gamedata`) reads. Data repositories depend on this crate only.

/// The data-contract version a package must be built for.
///
/// A data package is the runtime JSON under `data/<poe_version>/` that a data
/// repository (RePoE's `pobr-data-adapter` and release build) produces from
/// these types. Its `manifest.json` records the contract it was built for as
/// `contract_version`, and the loader refuses a package that declares any
/// other number (see `pobr-gamedata`'s manifest validation).
///
/// Bump this by 1 for a breaking change to any serialized type a data package
/// contains, or to how the loader interprets existing bytes: a renamed,
/// removed or retyped field, a field that becomes required, a changed enum
/// spelling, or new meaning for an existing value. An additive optional field
/// (`#[serde(default)]`, absent in old packages and meaning what its absence
/// meant before) does not bump it.
///
/// Released as immutable tags `contract/<n>.<m>`: `n` is this number, `m` an
/// additive revision (new optional fields, `pobr-data` helper fixes) starting
/// at 0. Data repositories pin their `pobr-data` dependency to the latest
/// `<n>.<m>` they need (docs/data-architecture.md §3.1).
pub const CONTRACT_VERSION: u32 = 1;

pub mod build_config;
pub mod catalog;
pub mod constants;
pub mod damage;
pub mod display_stat;
pub mod item;
pub mod minion;
pub mod modifier;
pub mod monster;
pub mod native_stat;
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
    pub use crate::item::*;
    pub use crate::minion::*;
    pub use crate::modifier::*;
    pub use crate::monster::*;
    pub use crate::passive_tree::*;
    pub use crate::skill::*;
    pub use crate::source::*;
    pub use crate::stat::*;
}
