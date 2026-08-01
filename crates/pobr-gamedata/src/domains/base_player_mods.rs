//! `base/base_player_mods.json` loader (player-inherent baseline mods,
//! sourced from PoB2 `CalcSetup.lua`'s `initModDB` + initEnv's baseline
//! section 608-678).
//!
//! Loaded via `load_domain` (`base/` first, falling back to the version
//! root, see [`crate::paths`]). This table only carries entries pobr's
//! existing Rust source of truth already has a value for (a migration
//! invariant); see `tests/load_base_player_mods.rs` for the
//! entry-by-entry comparison test.

use pobr_data::catalog::base_player_mods::BasePlayerModDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the player-inherent baseline mod table (keeps the file's
    /// order: grouped by vendor's injection order).
    pub fn base_player_mods(&self) -> Result<Vec<BasePlayerModDef>, LoadError> {
        self.load_domain("base_player_mods.json")
    }
}
