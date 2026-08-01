//! `base/game_constants.json` loader — the three global constant sections
//! character/monster/game, schema in
//! [`pobr_data::catalog::game_constants`].
//!
//! pobr's Rust source of truth is `crates/pobr-data/src/constants.rs`
//! (top-level consts + `GameConstants::poe2()`, matching value-for-value);
//! vendor-only fields are extracted from PoB2 `src/Data/Misc.lua` +
//! `src/Modules/Data.lua` (data.misc, L171-250).

use pobr_data::catalog::game_constants::GameConstantsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the global game constants table (via the `base/`-first,
    /// version-root-fallback domain location).
    pub fn game_constants(&self) -> Result<GameConstantsDef, LoadError> {
        self.load_domain("game_constants.json")
    }
}
