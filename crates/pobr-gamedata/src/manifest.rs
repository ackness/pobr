//! Manifest (data-pack envelope) loading: compatible with both v1 / v2 shapes.
//!
//! v2's ([`pobr_data::catalog::CATALOG_SCHEMA_VERSION`] = 2) `domains` is
//! the three-section `{base, overlay, generated}`; v1's flat `domains`
//! array is treated as entirely belonging to `base` at the deserialization
//! layer ([`pobr_data::catalog::DomainSections`]'s serde impl).
//! `manifest.json` always lives at the version root and doesn't
//! participate in `base/` location.

use pobr_data::catalog::DataManifest;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the data-pack envelope (a v1 flat `domains` is automatically
    /// filed under the `base` section).
    pub fn manifest(&self) -> Result<DataManifest, LoadError> {
        self.load_json_at(self.root().join("manifest.json"))
    }
}

#[cfg(test)]
mod tests {
    use crate::GameData;

    fn temp_manifest(tag: &str, json: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pobr-gamedata-manifest-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("manifest.json"), json).unwrap();
        dir
    }

    /// A v1 manifest (flat domains) loads, with every domain filed under the base section.
    #[test]
    fn loads_v1_manifest() {
        let dir = temp_manifest(
            "v1",
            r#"{"schema_version":1,"poe_version":"4.5.0.0.0","languages":[],"domains":["stats","mods"]}"#,
        );
        let manifest = GameData::new(&dir).manifest().unwrap();
        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.domains.base, vec!["stats", "mods"]);
        assert!(manifest.domains.overlay.is_empty());
        assert!(manifest.domains.generated.is_empty());
    }

    /// A v2 manifest (three-section domains) loads.
    #[test]
    fn loads_v2_manifest() {
        let dir = temp_manifest(
            "v2",
            r#"{"schema_version":2,"poe_version":"4.5.0.0.0","languages":["zh-TW"],
                "domains":{"base":["stats"],"overlay":["skill_stat_map"],"generated":[]}}"#,
        );
        let manifest = GameData::new(&dir).manifest().unwrap();
        assert_eq!(manifest.schema_version, 2);
        assert_eq!(manifest.domains.base, vec!["stats"]);
        assert_eq!(manifest.domains.overlay, vec!["skill_stat_map"]);
        assert!(manifest.domains.generated.is_empty());
    }
}
