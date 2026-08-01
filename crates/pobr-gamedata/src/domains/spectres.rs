//! `overlay/spectres.json` loader — spectre entries (extracted from vendor
//! `Data/Spectres.lua` via `extract-lua --what spectres`, the same schema
//! as minions, keyed by the full metadata path; /A5).
//!
//! Size note: on the order of ~700KB, a lazily-loaded domain — only read
//! from disk when the consumer explicitly calls it, not on the default hot
//! path; `BuildData`'s wiring convention is minions first, falling back to
//! spectres on a miss.

use pobr_data::catalog::actors::MinionsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the spectre entry table (always resolved under `overlay/`).
    /// Returns `Ok(None)` when the file is missing (missing-table
    /// tolerance); other errors still propagate as usual.
    pub fn spectres(&self) -> Result<Option<MinionsDef>, LoadError> {
        match self.load_json_at::<MinionsDef>(self.overlay_path("spectres.json")) {
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
