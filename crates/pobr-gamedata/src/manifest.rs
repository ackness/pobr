//! Manifest loading and immutable runtime snapshot validation.
//!
//! Schemas 1/2 preserve legacy domain layouts; schema 3 adds required file hashes.
//! The manifest lives at the version root and is never overlaid by user patches.

use pobr_data::catalog::DataManifest;
use sha2::{Digest, Sha256};

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the data-pack envelope (a v1 flat `domains` is automatically
    /// filed under the `base` section).
    pub fn manifest(&self) -> Result<DataManifest, LoadError> {
        let path = self.root().join("manifest.json");
        let bytes = self.read_bytes(&path).map_err(|source| LoadError::Io {
            path: path.clone(),
            source,
        })?;
        serde_json::from_slice(&bytes).map_err(|source| LoadError::Parse { path, source })
    }

    /// Checks the immutable snapshot before any optional-domain fallbacks.
    /// Schemas 1/2 retain legacy missing-domain compatibility. Schema 3 requires
    /// every declared file and verifies original bytes before user patches merge.
    pub fn validate_manifest(&self) -> Result<DataManifest, LoadError> {
        let manifest = self.manifest()?;
        let invalid = |message| LoadError::Integrity {
            path: self.root().join("manifest.json"),
            message,
        };
        if !(1..=pobr_data::catalog::CATALOG_SCHEMA_VERSION).contains(&manifest.schema_version) {
            return Err(invalid(format!(
                "unsupported schema {}",
                manifest.schema_version
            )));
        }
        if manifest.poe_version.trim().is_empty() {
            return Err(invalid("empty data version".into()));
        }
        if manifest.schema_version < 3 {
            return Ok(manifest);
        }
        if manifest.files.is_empty() {
            return Err(invalid(
                "schema 3 requires a nonempty file inventory".into(),
            ));
        }
        for (file, digest) in &manifest.files {
            let parts: Vec<_> = file.split('/').collect();
            if parts.len() < 2
                || !matches!(parts[0], "base" | "overlay" | "generated" | "i18n")
                || parts
                    .iter()
                    .any(|part| part.is_empty() || *part == "." || *part == "..")
                || file.contains(['\\', ':'])
                || !file.ends_with(".json")
            {
                return Err(invalid(format!("unsafe snapshot path: {file}")));
            }
            if digest.len() != 64
                || !digest
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            {
                return Err(invalid(format!("invalid SHA-256 for {file}")));
            }
            let path = self.root().join(file);
            let bytes = self
                .read_bytes(&path)
                .map_err(|source| LoadError::Io { path, source })?;
            if format!("{:x}", Sha256::digest(&bytes)) != *digest {
                return Err(invalid(format!("SHA-256 mismatch: {file}")));
            }
        }
        for (section, domains) in [
            ("base", &manifest.domains.base),
            ("overlay", &manifest.domains.overlay),
            ("generated", &manifest.domains.generated),
        ] {
            for domain in domains {
                if !manifest
                    .files
                    .contains_key(&format!("{section}/{domain}.json"))
                {
                    return Err(invalid(format!(
                        "declared domain lacks an inventory entry: {section}/{domain}"
                    )));
                }
            }
        }
        for language in &manifest.languages {
            let prefix = format!("i18n/{language}/");
            if !manifest.files.keys().any(|file| file.starts_with(&prefix)) {
                return Err(invalid(format!(
                    "declared language lacks files: {language}"
                )));
            }
        }
        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use crate::GameData;
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;

    fn strict_files() -> BTreeMap<String, Vec<u8>> {
        let quality = br#"{"effects":[]}"#.to_vec();
        let manifest = serde_json::json!({
            "schema_version": 3, "poe_version": "test", "languages": [],
            "domains": { "base": [], "overlay": ["gem_quality_stats"], "generated": [] },
            "files": { "overlay/gem_quality_stats.json": format!("{:x}", Sha256::digest(&quality)) }
        });
        BTreeMap::from([
            (
                "manifest.json".into(),
                serde_json::to_vec(&manifest).unwrap(),
            ),
            ("overlay/gem_quality_stats.json".into(), quality),
        ])
    }

    #[test]
    fn strict_snapshot_rejects_missing_or_changed_quality() {
        let files = strict_files();
        GameData::from_memory(files.clone())
            .validate_manifest()
            .unwrap();
        let mut missing = files.clone();
        missing.remove("overlay/gem_quality_stats.json");
        assert!(GameData::from_memory(missing).validate_manifest().is_err());
        let mut changed = files;
        changed.insert("overlay/gem_quality_stats.json".into(), b"{}".to_vec());
        let err = GameData::from_memory(changed)
            .validate_manifest()
            .unwrap_err();
        assert!(err.to_string().contains("SHA-256 mismatch"));
    }

    #[test]
    fn strict_snapshot_validates_original_bytes_before_user_patch() {
        let mut files = strict_files();
        files.insert(
            "patch/overlay/gem_quality_stats.json".into(),
            b"{\"effects\":[]}".to_vec(),
        );
        GameData::from_memory(files).validate_manifest().unwrap();
    }

    #[test]
    fn manifest_rejects_unknown_schema_empty_inventory_and_path_traversal() {
        for (schema, file) in [(99, None), (3, None), (3, Some("base/../../secret.json"))] {
            let files = file
                .map(|file| BTreeMap::from([(file, "0".repeat(64))]))
                .unwrap_or_default();
            let manifest = serde_json::json!({
                "schema_version": schema, "poe_version": "test", "languages": [],
                "domains": { "base": [] }, "files": files
            });
            let data = GameData::from_memory(BTreeMap::from([(
                "manifest.json".into(),
                serde_json::to_vec(&manifest).unwrap(),
            )]));
            assert!(data.validate_manifest().is_err());
        }
    }

    #[test]
    fn legacy_snapshot_keeps_optional_domain_compatibility() {
        let manifest = br#"{"schema_version":2,"poe_version":"old","languages":[],"domains":{"base":[],"overlay":["gem_quality_stats"]}}"#;
        let data = GameData::from_memory(BTreeMap::from([(
            "manifest.json".into(),
            manifest.to_vec(),
        )]));
        data.validate_manifest().unwrap();
        assert!(data.gem_quality_stats().unwrap().is_none());
    }

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
