//! `overlay/local_mods.json` loader — the local-mod whitelist (weapon
//! local `increased` suffixes + the `adds`-shaped damage suffix), schema
//! in [`pobr_data::catalog::local_mods`].
//!
//! pobr's source of truth is the original hardcoded enum in
//! `pobr-build::calc_orchestrator::is_weapon_local_mod` (a migration
//! invariant); the consumer `BuildData::load` degrades to
//! `LocalModsDef::default()` (a built-in mirror value-equal to this file)
//! when the file is missing.

use pobr_data::catalog::local_mods::LocalModsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the local-mod whitelist (a single-object, version-independent
    /// curated domain): version `overlay/` first, `overlay-common/` as the
    /// fallback ([`Self::load_overlay_or_common`]); errors if both layers
    /// are missing — degradation semantics are the consumer's call, not
    /// swallowed at the loader layer.
    pub fn local_mods(&self) -> Result<LocalModsDef, LoadError> {
        self.load_overlay_or_common("local_mods.json")
    }
}
