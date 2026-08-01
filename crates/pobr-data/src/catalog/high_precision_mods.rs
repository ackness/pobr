//! Rounding-precision exception table domain schema
//! (`overlay/high_precision_mods.json`).
//!
//! Data source (vendor commit `2df5a74`, see `vendor/.pob2-version.txt`):
//! - `src/Modules/Data.lua:413`'s `data.defaultHighPrecision = 1`;
//! - `src/Modules/Data.lua:415-530`'s `data.highPrecisionMods` (38 entries
//!   of `mod name → mod type → precision digit count`).
//!
//! Vendor consumption points (this table's intended wiring targets,
//! **none implemented in pobr yet**):
//! - `src/Classes/ModStore.lua:45-81`'s `ScaleAddMod`: the rounding
//!   precision for a scaled value — a hit in this table uses
//!   `floor(v·10^p)/10^p`, a miss that still produces a fraction uses
//!   `defaultHighPrecision`, otherwise `modf(round(v, 2))` takes the
//!   integer part;
//! - `src/Classes/ModList.lua:118-147`'s `MoreInternal`: rounding per
//!   modName in the MORE product — a hit in this table uses
//!   `floor(result·10^p)/10^p`, a miss uses `round(modResult, 2)`.
//!
//! pobr's current state:
//! - `pobr-core::mod_db::round_more` is hardcoded to `round(·, 2)`, with
//!   **no exception-table branch** (matches vendor's `MoreInternal`
//!   default branch value-for-value);
//! - the ScaleAddMod primitive isn't implemented at all. So this
//!   table currently **has zero consumers and zero parity impact** — it's
//!   stored now, to be wired up as injected data once ScaleAddMod / the
//!   MORE exception branch land.
//! - `more_default_round_decimals` is the only field with a pobr source of
//!   truth (= round_more's 2).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Top level of `overlay/high_precision_mods.json` (a single-object domain).
///
/// Precision semantics: `p` digits means `p` decimal places are kept
/// (`floor(v × 10^p) / 10^p` — vendor uses `floor`, not round-half-up).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighPrecisionModsDef {
    /// The default precision digit count used when a scaled result has a
    /// fraction but misses the exception table (vendor
    /// `Data.lua:413`'s `defaultHighPrecision = 1`; only ScaleAddMod consumes it).
    pub default_high_precision: u32,
    /// The default rounding decimal count per modName in the MORE product
    /// (vendor `ModList.lua:144`'s `round(modResult, 2)`'s 2; pobr's
    /// source of truth: `pobr-core::mod_db::round_more` is hardcoded to 2,
    /// matching value-for-value).
    pub more_default_round_decimals: u32,
    /// The precision exception table: `mod name → (mod type → precision digit count)`.
    ///
    /// Mod type uses vendor's raw literal `"BASE"` / `"MORE"` (aligning it
    /// with pobr's `ModType` serialization name is left to the wiring-up
    /// wave's decision; this table transcribes vendor's keys faithfully).
    pub mods: BTreeMap<String, BTreeMap<String, u32>>,
}
