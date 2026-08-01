//! `overlay/special_mods.json` loader — special mod-line templates (a
//! batched data-driving of vendor ModParser's `specialModList`, a
//! hand-curated domain; schema in [`pobr_data::catalog::parser_rules`],
//! /B-4).
//!
//! Consumer: the `RuleSet.special_mods` domain → `CalcOrchestrator` compiles
//! `SpecialModRules::compile` once at build time → every ingest path goes
//! through `parse_mod_with_rules`. At that point,
//! `generated/special_derived.json` (keystone-derived) is concatenated
//! with this table's entries, and an id collision is an error.

use pobr_data::catalog::parser_rules::SpecialModsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the special mod-line template table, **merging two layers**:
    /// first reads the version-independent curation layer
    /// `data/overlay-common/special_mods.json` (skipped if absent), then
    /// layers the version layer `<root>/overlay/special_mods.json` on top.
    /// The merge semantics are simple: **a version-layer entry overrides
    /// the common layer's same-id entry, everything else is appended in
    /// appearance order** (common's order is preserved, version-only
    /// entries appended after). Hand-curated vendor-semantics fixes go in
    /// the common layer, so new data-version directories inherit them
    /// automatically, without a per-version manual migration
    /// (`docs/version-bump-architecture.md` P1-3).
    ///
    /// Both layers missing → `Ok(None)` (convention: a missing table →
    /// the RuleSet domain is None, parsing falls back to the hardcoded
    /// path). Either layer having broken JSON still propagates as usual; a
    /// missing common-layer file is the normal case (a soft degradation,
    /// not an error).
    pub fn special_mods(&self) -> Result<Option<SpecialModsDef>, LoadError> {
        let common = match self.overlay_common_path("special_mods.json") {
            Some(path) => self.load_special_mods_at(path)?,
            None => None,
        };
        let version = self.load_special_mods_at(self.overlay_path("special_mods.json"))?;
        Ok(match (common, version) {
            (None, None) => None,
            (Some(def), None) | (None, Some(def)) => Some(def),
            (Some(common), Some(version)) => Some(merge_special_layers(common, version)),
        })
    }

    /// Reads a `special_mods`-schema file as an `Option`: NotFound → `None`
    /// (a soft degradation), other errors propagate. `load_json_at` still
    /// layers the user patch on top.
    fn load_special_mods_at(
        &self,
        path: std::path::PathBuf,
    ) -> Result<Option<SpecialModsDef>, LoadError> {
        match self.load_json_at::<SpecialModsDef>(path) {
            Ok(def) => Ok(Some(def)),
            Err(LoadError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// Loads the keystone-derived special table
    /// (`generated/special_derived.json`, adapter output; same schema as
    /// `special_mods/v1`). A missing table returns `Ok(None)` (a
    /// transition state before C-1 lands); broken JSON still propagates.
    pub fn special_derived(&self) -> Result<Option<SpecialModsDef>, LoadError> {
        match self.load_json_at::<SpecialModsDef>(self.generated_path("special_derived.json")) {
            Ok(def) => Ok(Some(def)),
            Err(LoadError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// Loads the batch-vendor-extracted special table
    /// (`generated/special_vendor.json`, output of
    /// `sync-pob-catalog extract-lua --what special-mods`, batch V0; same
    /// schema as `special_mods/v1`). A missing table returns `Ok(None)`;
    /// broken JSON still propagates.
    pub fn special_vendor(&self) -> Result<Option<SpecialModsDef>, LoadError> {
        match self.load_json_at::<SpecialModsDef>(self.generated_path("special_vendor.json")) {
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

/// Merges the two curation layers: `common` (a version-independent
/// baseline) with `version` (version-specific overrides) layered on top,
/// overridden/appended entry by entry by `id` (a special_mods
/// specialization of [`crate::paths::merge_by_key`]).
fn merge_special_layers(common: SpecialModsDef, version: SpecialModsDef) -> SpecialModsDef {
    SpecialModsDef {
        entries: crate::paths::merge_by_key(common.entries, version.entries, |e| &e.id),
    }
}
