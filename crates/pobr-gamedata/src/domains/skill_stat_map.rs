//! `overlay/skill_stat_map.json` loader — SkillStatMap's global +
//! per-statSet overrides (stat id → modifier-constructor mapping), schema
//! in [`pobr_data::catalog::stat_map`]
//!
//! Data source: vendor PoB2 `Data/SkillStatMap.lua` plus the `statMap`
//! field of each statSet in `Data/Skills/*.lua`, deterministically
//! extracted by `sync-pob-catalog extract-lua --what stat-map` (schema id
//! `skill_stat_map/v1`). Loaded as a whole domain; consumer =
//! `pobr-core::rules::stat_map_engine` (the merge formula + ModName
//! translation + tag-support checks — this loader has zero semantics).

use pobr_data::catalog::stat_map::SkillStatMapDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the SkillStatMap mapping table (always resolved under
    /// `overlay/`). Returns `Ok(None)` when the file is missing (an old
    /// data pack without this overlay domain) — the consumer behaves as
    /// "the data engine is unavailable" (the dual-run framework can only
    /// use Legacy, backward compatible); other I/O / parse errors still
    /// propagate, not silenced.
    pub fn skill_stat_map(&self) -> Result<Option<SkillStatMapDef>, LoadError> {
        match self.load_json_at::<SkillStatMapDef>(self.overlay_path("skill_stat_map.json")) {
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
            "pobr-gamedata-stat-map-{tag}-{}",
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
        let loaded = GameData::new(&dir).skill_stat_map().unwrap();
        assert!(loaded.is_none());
    }

    /// Normal load: the `_meta` header is ignored, both the global /
    /// per_stat_set sections read out, merge parameters and mod-constructor
    /// field shapes match the schema.
    #[test]
    fn loads_global_and_per_set_and_ignores_meta() {
        let dir = temp_dir("loads");
        std::fs::create_dir_all(dir.join("overlay")).unwrap();
        std::fs::write(
            dir.join("overlay/skill_stat_map.json"),
            r#"{
              "_meta": { "schema": "skill_stat_map/v1" },
              "global": {
                "total_cast_time_+_ms": {
                  "div": 1000.0,
                  "mods": [ { "kind": "mod", "name": "TotalCastTime", "mod_type": "BASE" } ]
                },
                "broken_entry": { "_unextractable": true }
              },
              "per_stat_set": {
                "IceNovaPlayer": {
                  "1": {
                    "damage_+%": {
                      "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC" } ]
                    }
                  }
                }
              }
            }"#,
        )
        .unwrap();
        let def = GameData::new(&dir)
            .skill_stat_map()
            .unwrap()
            .expect("overlay exists, should load");
        let entry = &def.global["total_cast_time_+_ms"];
        assert_eq!(entry.div, Some(1000.0));
        assert_eq!(entry.mods[0].name.as_deref(), Some("TotalCastTime"));
        assert_eq!(entry.mods[0].mod_type.as_deref(), Some("BASE"));
        assert!(def.global["broken_entry"].unextractable);
        assert!(def.per_stat_set["IceNovaPlayer"]["1"].contains_key("damage_+%"));
    }

    /// Invalid JSON → a Parse error, not silenced.
    #[test]
    fn malformed_json_errors_out() {
        let dir = temp_dir("malformed");
        std::fs::create_dir_all(dir.join("overlay")).unwrap();
        std::fs::write(dir.join("overlay/skill_stat_map.json"), "{ not json").unwrap();
        assert!(GameData::new(&dir).skill_stat_map().is_err());
    }

    /// A real-data-pack smoke test: the repo's 4.5.0.3.4 overlay
    /// deserializes as a whole, at a scale matching vendor's order of
    /// magnitude (global 950+, per-set covering 300+ effects).
    #[test]
    fn loads_repo_overlay_smoke() {
        let root = crate::current_data_dir();
        if !root.join("overlay/skill_stat_map.json").exists() {
            return; // Skipped when the data pack isn't in place (avoids a
            // false report when CI is missing data)
        }
        let def = GameData::new(&root)
            .skill_stat_map()
            .unwrap()
            .expect("repo overlay exists");
        assert!(def.global.len() >= 950, "global={}", def.global.len());
        assert!(
            def.per_stat_set.len() >= 300,
            "per_set effects={}",
            def.per_stat_set.len()
        );
    }
}
