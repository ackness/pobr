//! `overlay/mod_scalability.json` loader — the scalability + format
//! conversion table for `{range:x}` mod values (extracted from vendor
//! `Data/ModScalability.lua` via `extract-lua --what mod-scalability`,
//! schema in [`pobr_data::catalog::item_overlay`]).
//!
//! Size note: on the order of ~4MB, a lazily-loaded domain; consumer
//! (`pobr-core::apply_range`) injects it via RuleSet `ItemRules`, zero
//! wiring here.

use pobr_data::catalog::item_overlay::ModScalabilityDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the scalability table (always resolved under `overlay/`).
    /// Returns `Ok(None)` when the file is missing (the consumer degrades
    /// to naive linear value resolution plus an `approx` flag); other
    /// errors still propagate as usual.
    pub fn mod_scalability(&self) -> Result<Option<ModScalabilityDef>, LoadError> {
        match self.load_json_at::<ModScalabilityDef>(self.overlay_path("mod_scalability.json")) {
            Ok(def) => Ok(Some(def)),
            Err(LoadError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
}
