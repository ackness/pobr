//! `overlay/curse_priority.json` loader — the curse priority data table
//! (vendor `Modules/Data.lua:274`'s `data.cursePriority` plain data table,
//! extracted via `extract-lua --what curse-priority`, schema in
//! [`pobr_data::catalog::curse_priority`], M6-C).
//!
//! Consumer (`calc/buff_pass.rs`'s curse priority/limit, matching
//! `determineCursePriority` in CalcPerform.lua:454-485) is wired in
//! separately; this loader has zero wiring.

use pobr_data::catalog::curse_priority::CursePriorityDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the curse priority table (always resolved under `overlay/`;
    /// `_meta` is ignored by serde). Returns `Ok(None)` when the file is
    /// missing (missing-table tolerance, the consumer falls back to the
    /// old path); other errors still propagate as usual.
    pub fn curse_priority(&self) -> Result<Option<CursePriorityDef>, LoadError> {
        match self.load_json_at::<CursePriorityDef>(self.overlay_path("curse_priority.json")) {
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
