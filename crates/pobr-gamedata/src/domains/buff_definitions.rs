//! `overlay/buff_definitions.json` loader — built-in buff definitions (a
//! hand-curated overlay exception for doActorMisc), schema in
//! [`pobr_data::catalog::buffs`].
//!
//! The drift guardrail lives on the tooling side:
//! `sync-pob-catalog check-buff-refs` reconciles vendor_ref line-range
//! hashes; consumer = `pobr-core::rules::buff_expander`.

use pobr_data::catalog::buffs::BuffDefinitionsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads built-in buff definitions, **merging two layers**: the
    /// version-independent curation layer
    /// `data/overlay-common/buff_definitions.json` (hand-curated,
    /// providing the baseline), with the version layer
    /// `<root>/overlay/buff_definitions.json` (version-specific overrides)
    /// layered on top. Merged entry by entry by `id`
    /// ([`crate::paths::merge_by_key`], same as special_mods) — the
    /// hand-curated buff semantics don't change with the game version, so
    /// keeping them in the common layer lets new versions inherit them for
    /// free (`docs/version-bump-architecture.md` P1-3); `_meta` is
    /// ignored by serde.
    ///
    /// Both layers missing (an old data pack without this overlay domain)
    /// returns `Ok(None)` — the consumer behaves as "no built-in buff
    /// expansion" (backward compatible); other I/O / parse errors still
    /// propagate, not silenced.
    pub fn buff_definitions(&self) -> Result<Option<BuffDefinitionsDef>, LoadError> {
        let common = match self.overlay_common_path("buff_definitions.json") {
            Some(path) => self.load_buff_definitions_at(path)?,
            None => None,
        };
        let version = self.load_buff_definitions_at(self.overlay_path("buff_definitions.json"))?;
        Ok(match (common, version) {
            (None, None) => None,
            (Some(def), None) | (None, Some(def)) => Some(def),
            (Some(common), Some(version)) => Some(BuffDefinitionsDef {
                buffs: crate::paths::merge_by_key(common.buffs, version.buffs, |b| &b.id),
            }),
        })
    }

    /// Reads a `buff_definitions`-schema file as an `Option`: NotFound →
    /// `None` (a soft degradation), other errors propagate.
    /// `load_json_at` still layers the user patch on top.
    fn load_buff_definitions_at(
        &self,
        path: std::path::PathBuf,
    ) -> Result<Option<BuffDefinitionsDef>, LoadError> {
        match self.load_json_at::<BuffDefinitionsDef>(path) {
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
