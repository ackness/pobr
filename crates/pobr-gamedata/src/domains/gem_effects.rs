//! `overlay/gem_effects.json` loader — gem → granted-effect edges
//! (`gem_id → {granted_effect_id, additional_granted_effect_ids, additional_stat_set_ids}`),
//! schema in [`pobr_data::catalog::skills`]'s `GemEffectDef` section (the
//! data-plane + contract C5 source of `SkillGemDef`'s edges).
//!
//! Data source: vendor PoB2 `Data/Gems.lua` (the export product of the
//! `.dat`'s `GemEffects` table, whose bundle isn't downloadable at the
//! pinned patch), deterministically extracted by
//! `sync-pob-catalog extract-lua --what gem-effects` (schema id `gem_effects/v1`).
//!
//! Consumption: [`crate::GameData::skill_gems`] merges this by `gem_id`
//! after loading the base gem table, filling in
//! `SkillGemDef::granted_effect_id` / `additional_granted_effect_ids`;
//! pobr-build's `BuildData` separately indexes it by `granted_effect_id`
//! for the meta expansion (T5.6).

use pobr_data::catalog::GemEffectsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the gem → effect edge table (always resolved under
    /// `overlay/`). Returns `Ok(None)` when the file is missing (an old
    /// data pack without this overlay domain) — the consumer behaves as
    /// "the edge fields stay empty" (backward compatible); other I/O /
    /// parse errors still propagate, not silenced.
    pub fn gem_effects(&self) -> Result<Option<GemEffectsDef>, LoadError> {
        match self.load_json_at::<GemEffectsDef>(self.overlay_path("gem_effects.json")) {
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

#[cfg(test)]
mod tests {
    use crate::GameData;

    /// Creates a temp version directory with a unique name.
    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pobr-gamedata-gem-effects-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A missing overlay file (an old data pack) → Ok(None), no error.
    #[test]
    fn missing_overlay_file_yields_none() {
        let dir = temp_dir("missing");
        let loaded = GameData::new(&dir).gem_effects().unwrap();
        assert!(loaded.is_none());
    }

    /// Normal load: the `_meta` header is ignored, the gems list read out in order.
    #[test]
    fn loads_gems_and_ignores_meta() {
        let dir = temp_dir("loads");
        std::fs::create_dir_all(dir.join("overlay")).unwrap();
        std::fs::write(
            dir.join("overlay/gem_effects.json"),
            r#"{
              "_meta": { "schema": "gem_effects/v1" },
              "gems": [
                { "gem_id": "Metadata/Items/Gems/SkillGemIceNova",
                  "variant_id": "IceNova",
                  "granted_effect_id": "IceNovaPlayer",
                  "additional_stat_set_ids": ["IceNovaPlayerOnFrostbolt"] }
              ]
            }"#,
        )
        .unwrap();
        let def = GameData::new(&dir)
            .gem_effects()
            .unwrap()
            .expect("overlay exists, should load");
        assert_eq!(def.gems.len(), 1);
        assert_eq!(def.gems[0].granted_effect_id, "IceNovaPlayer");
        assert!(def.gems[0].additional_granted_effect_ids.is_empty());
        assert_eq!(
            def.gems[0].additional_stat_set_ids,
            ["IceNovaPlayerOnFrostbolt"]
        );
    }

    /// Invalid JSON → a Parse error, not silenced.
    #[test]
    fn malformed_json_errors_out() {
        let dir = temp_dir("malformed");
        std::fs::create_dir_all(dir.join("overlay")).unwrap();
        std::fs::write(dir.join("overlay/gem_effects.json"), "{ not json").unwrap();
        assert!(GameData::new(&dir).gem_effects().is_err());
    }
}
