//! `base/non_damaging_ailments.json` loader (chill/freeze/shock magnitude
//! bounds + buildupTypes + defaultAilmentDamageTypes, sourced from PoB2
//! `Modules/Data.lua:347-410`).

use pobr_data::catalog::non_damaging_ailments::NonDamagingAilmentsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the non-damaging ailment table (`base/` first, falling back
    /// to the version root, see [`crate::paths`]).
    ///
    /// Values are value-equal to `pobr_data::monster`/`pobr_data::constants`'s
    /// existing consts (a migration invariant); the formulas still live in
    /// `pobr-core::calc::ailment` — W3 wires up consuming this table.
    pub fn non_damaging_ailments(&self) -> Result<NonDamagingAilmentsDef, LoadError> {
        self.load_domain("non_damaging_ailments.json")
    }
}
