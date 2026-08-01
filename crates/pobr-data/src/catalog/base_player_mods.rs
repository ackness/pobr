//! Player-inherent baseline mod domain schema (`base/base_player_mods.json`).
//!
//! Corresponds to the inherent baseline mods PoB2 injects when initializing
//! the player modDB:
//! - `vendor/PathOfBuilding-PoE2/src/Modules/CalcSetup.lua:19-105`
//!   (`initModDB`, charge caps, etc.);
//! - `vendor/PathOfBuilding-PoE2/src/Modules/CalcSetup.lua:608-678`
//!   (initEnv's player-baseline section).
//!
//! **Migration invariant**: this table only carries entries pobr's existing
//! Rust code already has a value for, and the JSON value must be
//! value-equal to the Rust source of truth; entries vendor has that pobr
//! doesn't (e.g. `DotMultiplier`/`MaximumRage`/`ActiveTrapLimit`/the
//! Tailwind section, roughly 60 entries) **aren't stored here** — left for
//! a later behavior-alignment commit to add. Each entry's Rust source of
//! truth and vendor line number are recorded where
//! `data/<version>/base/base_player_mods.json` is generated from (the
//! entry-by-entry comparison test is
//! `crates/pobr-gamedata/tests/load_base_player_mods.rs`).
//!
//! Type-reuse note: `mod_type` directly reuses [`crate::modifier::ModType`]
//! (already serde-able); `flags`/`keyword_flags` serialize as bit values
//! (u64), with the bit definitions matching
//! [`crate::modifier::ModFlags`] / [`crate::modifier::KeywordFlags`]; since
//! pobr-core's `ModTag` isn't in this crate and has non-serializable
//! variants, this module defines its own serializable minimal subset,
//! [`BasePlayerModTag`].

use serde::{Deserialize, Serialize};

use crate::modifier::ModType;

/// A player-inherent baseline mod entry (converted to a `Modifier` and
/// injected into the player ModDb by the setup/orchestration layer after
/// loading).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BasePlayerModDef {
    /// Stable entry ID (lowercase with underscores; used for the
    /// attribution source id and to locate this entry in the
    /// entry-by-entry comparison test).
    pub id: String,
    /// ModName (calc's internal stable name, e.g. `MaximumLife` /
    /// `FireResistance`).
    ///
    /// Note: some entries' names differ from vendor's (pobr uses
    /// `FireResistance`, vendor uses `FireResist`; pobr uses
    /// `MaximumLife`, vendor uses `Life`) — pobr's existing naming is kept,
    /// per the migration invariant.
    pub mod_name: String,
    /// Modifier type (`Base` / `Inc` / `More` / `Flag` / `Override` / `List`).
    pub mod_type: ModType,
    /// Value (a `Flag` entry uses `1.0` as true, by convention).
    pub value: f64,
    /// ModFlags bit value (matches [`crate::modifier::ModFlags`]; 0 = no
    /// restriction, omitted when serialized).
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub flags: u64,
    /// KeywordFlags bit value (matches [`crate::modifier::KeywordFlags`];
    /// 0 = no restriction, omitted when serialized).
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub keyword_flags: u64,
    /// Tag list (empty = applies unconditionally, omitted when serialized).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<BasePlayerModTag>,
}

/// A serializable tag for a baseline mod (a minimal serde subset of
/// pobr-core's `ModTag` plus vendor's extension fields).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BasePlayerModTag {
    /// Scales linearly with some variable's count (PoB2's `Multiplier`
    /// tag, corresponds to pobr-core's `ModTag::Multiplier`).
    ///
    /// Effective value = `multiplier(var) / div × value + base`. `base` is
    /// a vendor extension constant term (e.g. `CalcSetup.lua:615`'s
    /// `Life BASE 12 {Multiplier, var=Level, base=16}`); pobr-core's
    /// `ModTag::Multiplier` doesn't have this field yet — currently
    /// implemented equivalently by a formula in `character.rs`
    /// (`12×Level+16`); wiring up the consumer side is a follow-up
    /// behavior-alignment task.
    Multiplier {
        var: String,
        /// How many units per scaling step (PoB2's `div`, default 1).
        #[serde(default = "default_div")]
        div: f64,
        /// Cap on the number of scaling steps (PoB2's `limit`; omitted
        /// when there's no cap).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<f64>,
        /// An extra constant term unrelated to the scaling (vendor's
        /// `base`; omitted when absent).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<f64>,
    },
    /// A boolean condition gate (PoB2's `Condition` tag, corresponds to
    /// pobr-core's `ModTag::Condition`).
    Condition {
        var: String,
        /// Negation (PoB2's `neg`; omitted when false).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        negated: bool,
    },
}

/// Default value for `div` (PoB2's Multiplier tag defaults to 1 when
/// there's no divisor).
fn default_div() -> f64 {
    1.0
}

/// serde predicate to skip a zero bit value (keeps diffs clean).
fn is_zero_u64(v: &u64) -> bool {
    *v == 0
}
