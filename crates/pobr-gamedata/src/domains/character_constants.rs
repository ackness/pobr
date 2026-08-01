//! `base/character_constants.json` loader (character level/attribute-derived
//! constants, values migrated out of `pobr-core::character`, sourced from
//! PoB2's `data.characterConstants`).

use pobr_data::catalog::character_constants::CharacterConstantsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the character base constants (a single-object domain: the
    /// whole file is one [`CharacterConstantsDef`]).
    ///
    /// Located via the three-layer directory lookup (`base/` first,
    /// falling back to the version root, see [`crate::paths`]).
    pub fn character_constants(&self) -> Result<CharacterConstantsDef, LoadError> {
        self.load_domain("character_constants.json")
    }
}
