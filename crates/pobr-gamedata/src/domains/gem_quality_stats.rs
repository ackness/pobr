//! `overlay/gem_quality_stats.json` loader — gem quality-stat slopes
//! (`effect_id → [{stat, per_quality_rate}]`), schema in
//! [`pobr_data::catalog::skills`]'s `GemQualityStatsDef` section.
//!
//! Data source: vendor PoB2 `Data/Skills/*.lua`'s `qualityStats` field,
//! deterministically extracted by
//! `sync-pob-catalog extract-lua --what gem-quality` (schema id
//! `gem_quality_stats/v1`). This table is a plain lookup table (no merge
//! semantics against a base with the same shape), loaded as a whole
//! domain, indexed by the consumer (pobr-build's `BuildData`).

use pobr_data::catalog::GemQualityStatsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the gem quality-stat table (always resolved under
    /// `overlay/`). Returns `Ok(None)` when the file is missing (an old
    /// data pack without this overlay domain) — the consumer behaves as
    /// "quality produces no stat" (backward compatible); other I/O /
    /// parse errors still propagate, not silenced.
    pub fn gem_quality_stats(&self) -> Result<Option<GemQualityStatsDef>, LoadError> {
        match self.load_json_at::<GemQualityStatsDef>(self.overlay_path("gem_quality_stats.json")) {
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
            "pobr-gamedata-gem-quality-{tag}-{}",
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
        let loaded = GameData::new(&dir).gem_quality_stats().unwrap();
        assert!(loaded.is_none());
    }

    /// Normal load: the `_meta` header is ignored, the effects list read out in order.
    #[test]
    fn loads_effects_and_ignores_meta() {
        let dir = temp_dir("loads");
        std::fs::create_dir_all(dir.join("overlay")).unwrap();
        std::fs::write(
            dir.join("overlay/gem_quality_stats.json"),
            r#"{
              "_meta": { "schema": "gem_quality_stats/v1" },
              "effects": [
                { "effect_id": "CometPlayer",
                  "stats": [ { "stat": "base_spell_%_chance_to_echo", "per_quality_rate": 0.5 } ] }
              ]
            }"#,
        )
        .unwrap();
        let def = GameData::new(&dir)
            .gem_quality_stats()
            .unwrap()
            .expect("overlay 存在应加载");
        assert_eq!(def.effects.len(), 1);
        assert_eq!(def.effects[0].effect_id, "CometPlayer");
        assert_eq!(def.effects[0].stats[0].stat, "base_spell_%_chance_to_echo");
        assert_eq!(def.effects[0].stats[0].per_quality_rate, 0.5);
    }

    /// Invalid JSON → a Parse error, not silenced.
    #[test]
    fn malformed_json_errors_out() {
        let dir = temp_dir("malformed");
        std::fs::create_dir_all(dir.join("overlay")).unwrap();
        std::fs::write(dir.join("overlay/gem_quality_stats.json"), "{ not json").unwrap();
        assert!(GameData::new(&dir).gem_quality_stats().is_err());
    }
}
