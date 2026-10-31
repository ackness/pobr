//! `overlay/stat_set_labels.json` loader — a statSet's form label + vendor
//! export index (`(skill, set_id) → {set_index, label}`), schema in
//! [`pobr_data::catalog::skills`]'s `StatSetLabelDef` section.
//!
//! Data source: vendor `Data/Skills/*.lua` (label text) joined with
//! `Export/Skills/*.txt` templates (set id / export index),
//! deterministically extracted by
//! `sync-pob-catalog extract-lua --what stat-set-labels`.
//!
//! Consumption: [`crate::GameData::skill_stat_sets`] merges this by
//! `(effect_id, set_id)` after loading the base multi-set domain, filling
//! in `vendor_set_index` and falling back to its label only when
//! `StatSetDef::label` was not resolved from the native table.

use pobr_data::catalog::StatSetLabelsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the statSet label table (always resolved under `overlay/`).
    /// Returns `Ok(None)` when the file is missing (an old data pack
    /// without this overlay domain) — native labels remain available and
    /// compatibility indices stay empty; other I/O / parse
    /// errors still propagate, not silenced.
    pub fn stat_set_labels(&self) -> Result<Option<StatSetLabelsDef>, LoadError> {
        match self.load_json_at::<StatSetLabelsDef>(self.overlay_path("stat_set_labels.json")) {
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
    use serde_json::json;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pobr-gamedata-stat-set-labels-{tag}-{}",
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
        assert!(GameData::new(&dir).stat_set_labels().unwrap().is_none());
    }

    /// Normal load: the `_meta` header is ignored, the labels list read out in order.
    #[test]
    fn loads_labels_and_ignores_meta() {
        let dir = temp_dir("loads");
        std::fs::create_dir_all(dir.join("overlay")).unwrap();
        std::fs::write(
            dir.join("overlay/stat_set_labels.json"),
            r#"{
              "_meta": { "schema": "stat_set_labels/v1" },
              "labels": [
                { "skill": "IceNovaPlayer", "set_id": "IceNovaColdInfusedPlayer",
                  "set_index": 2, "label": "Cold-Infused" }
              ]
            }"#,
        )
        .unwrap();
        let def = GameData::new(&dir)
            .stat_set_labels()
            .unwrap()
            .expect("overlay exists, should load");
        assert_eq!(def.labels.len(), 1);
        assert_eq!(def.labels[0].set_index, 2);
        assert_eq!(def.labels[0].label, "Cold-Infused");
    }

    #[test]
    fn native_labels_take_precedence_while_compatibility_indices_and_legacy_labels_survive() {
        let dir = temp_dir("native-precedence");
        std::fs::create_dir_all(dir.join("base")).unwrap();
        std::fs::create_dir_all(dir.join("overlay")).unwrap();
        let base = json!([{"effect_id": "ExamplePlayer", "sets": [
            {"set_id": "Primary", "label": "Native primary", "levels": []},
            {"set_id": "Legacy", "levels": []},
            {"set_id": "NativeOnly", "label": "Native additional", "levels": []}
        ]}]);
        std::fs::write(
            dir.join("base/granted_effect_stat_sets.json"),
            serde_json::to_vec(&base).unwrap(),
        )
        .unwrap();
        let overlay = json!({"labels": [
            {"skill": "ExamplePlayer", "set_id": "Primary", "set_index": 1, "label": "Old primary"},
            {"skill": "ExamplePlayer", "set_id": "Legacy", "set_index": 2, "label": "Legacy additional"}
        ]});
        std::fs::write(
            dir.join("overlay/stat_set_labels.json"),
            serde_json::to_vec(&overlay).unwrap(),
        )
        .unwrap();
        let definitions = GameData::new(&dir).skill_stat_sets().unwrap();
        let sets = &definitions[0].sets;
        assert_eq!(sets[0].label.as_deref(), Some("Native primary"));
        assert_eq!(sets[0].vendor_set_index, Some(1));
        assert_eq!(sets[1].label.as_deref(), Some("Legacy additional"));
        assert_eq!(sets[1].vendor_set_index, Some(2));
        assert_eq!(sets[2].label.as_deref(), Some("Native additional"));
        assert_eq!(sets[2].vendor_set_index, None);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
