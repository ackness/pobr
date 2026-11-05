//! `base/jewel_radii.json` loader — radius-jewel ring bands (the distance
//! multiplier + per-tree-version label/inner/outer bands), schema in
//! [`pobr_data::catalog::jewel_radii`].
//!
//! Sourced from PoB2 `src/Modules/Data.lua:595-611` + `src/Data/Misc.lua:36`.
//! `crates/pobr-tree/src/radius_jewel.rs` reads radii from this table (no
//! engine copy); `JewelRadiiDef::default` is only the fallback for a table
//! that lacks a band.

use pobr_data::catalog::jewel_radii::JewelRadiiDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the radius-jewel ring-band table (via the `base/`-first,
    /// version-root-fallback domain location).
    pub fn jewel_radii(&self) -> Result<JewelRadiiDef, LoadError> {
        self.load_domain("jewel_radii.json")
    }
}
