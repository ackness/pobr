//! `overlay/high_precision_mods.json` loader — the rounding-precision
//! exception table (`defaultHighPrecision` + `mod name → mod type →
//! precision digit count`), schema in
//! [`pobr_data::catalog::high_precision_mods`].
//!
//! Sourced from vendor PoB2 `src/Modules/Data.lua:413-530`; the
//! consumption points (ScaleAddMod / the MORE precision exception branch)
//! aren't implemented in pobr yet — this loader lands with the data first,
//! zero wiring.

use pobr_data::catalog::high_precision_mods::HighPrecisionModsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the rounding-precision exception table (a single-object,
    /// version-independent curated domain): version `overlay/` first,
    /// `overlay-common/` as the fallback
    /// ([`Self::load_overlay_or_common`]), errors if both layers are missing.
    pub fn high_precision_mods(&self) -> Result<HighPrecisionModsDef, LoadError> {
        self.load_overlay_or_common("high_precision_mods.json")
    }
}
