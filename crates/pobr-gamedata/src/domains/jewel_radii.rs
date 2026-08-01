//! `base/jewel_radii.json` loader — radius-jewel ring bands (the distance
//! multiplier + per-tree-version label/inner/outer bands), schema in
//! [`pobr_data::catalog::jewel_radii`].
//!
//! Sourced from PoB2 `src/Modules/Data.lua:595-611` + `src/Data/Misc.lua:36`;
//! pobr's Rust source of truth is `crates/pobr-tree/src/radius_jewel.rs`
//! (the 4 named bands match value-for-value).

use pobr_data::catalog::jewel_radii::JewelRadiiDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the radius-jewel ring-band table (via the `base/`-first,
    /// version-root-fallback domain location).
    pub fn jewel_radii(&self) -> Result<JewelRadiiDef, LoadError> {
        self.load_domain("jewel_radii.json")
    }
}
