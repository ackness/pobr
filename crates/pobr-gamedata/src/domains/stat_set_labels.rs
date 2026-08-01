//! `overlay/stat_set_labels.json` loader — a statSet's form label + vendor
//! export index (`(skill, set_id) → {set_index, label}`), schema in
//! [`pobr_data::catalog::skills`]'s `StatSetLabelDef` section.
//!
//! Data source: vendor `Data/Skills/*.lua` (label text) joined with
//! `Export/Skills/*.txt` templates (set id / export index),
//! deterministically extracted by
//! `sync-pob-catalog extract-lua --what stat-set-labels` (the `.dat`
//! `Label` column's FK target table `GrantedEffectLabels` isn't
//! downloadable at the pinned patch).
//!
//! Consumption: [`crate::GameData::skill_stat_sets`] merges this by
//! `(effect_id, set_id)` after loading the base multi-set domain, filling
//! in `StatSetDef::label` / `vendor_set_index`.

use pobr_data::catalog::StatSetLabelsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the statSet label table (always resolved under `overlay/`).
    /// Returns `Ok(None)` when the file is missing (an old data pack
    /// without this overlay domain) — the consumer behaves as "label /
    /// export index stay empty" (backward compatible); other I/O / parse
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
}
