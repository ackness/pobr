//! `overlay/trigger_configs.json` loader — the 61 trigger configs from
//! vendor `Modules/CalcTriggers.lua`'s configTable (generated embedded in
//! `sync-pob-catalog gen-trigger-configs` plus a vendor-key scan
//! reconciliation; schema in [`pobr_data::catalog::triggers`]).
//!
//! Consumer: pobr-build's `BuildData::load` projects it into a recognition
//! table indexed by `match_effect_ids`, which the orchestrator's trigger
//! section uses to recognize gem-link/triggeredBy relationships; entries
//! with real logic are looked up in the registry by `handler_id` (zero
//! wiring here, count-monitored <100).

use pobr_data::catalog::triggers::TriggerConfigsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the trigger config table (always resolved under `overlay/`).
    /// Returns `Ok(None)` when the file is missing (missing-table
    /// tolerance; an old data pack's recognition surface stays empty =
    /// unchanged behavior); other errors still propagate as usual.
    pub fn trigger_configs(&self) -> Result<Option<TriggerConfigsDef>, LoadError> {
        match self.load_json_at::<TriggerConfigsDef>(self.overlay_path("trigger_configs.json")) {
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
