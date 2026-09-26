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
        self.snapshot_manifest()?
            .cloned()
            .ok_or_else(|| LoadError::Io {
                path: self.root().join("manifest.json"),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "missing manifest"),
            })
    }

    /// The manifest is optional for legacy standalone domain loaders. Present
    /// manifests are validated once, including all hashes, before any domain can
    /// degrade to an optional fallback. Clones share the same immutable inventory.
    pub(crate) fn snapshot_manifest(&self) -> Result<Option<&DataManifest>, LoadError> {
        match self.snapshot.get_or_init(|| match self.manifest() {
            Err(LoadError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                Ok(None)
            }
            Err(error) => Err(error.to_string()),
            Ok(manifest) => self
                .validate_snapshot(manifest)
                .map(Some)
                .map_err(|error| error.to_string()),
        }) {
            Ok(manifest) => Ok(manifest.as_ref()),
            Err(message) => Err(LoadError::Integrity {
                path: self.root().join("manifest.json"),
                message: message.clone(),
            }),
        }
    }

    pub(crate) fn snapshot_allows(&self, path: &std::path::Path) -> Result<bool, LoadError> {
        let Some(manifest) = self.snapshot_manifest()? else {
            return Ok(true);
        };
        if manifest.schema_version < 3 {
            return Ok(true);
        }
        // Shared curation is outside the sealed version directory on disk,
        // and uses the same relative key in the in-memory backend.
        if self
            .overlay_common_path("")
            .is_some_and(|common| path.starts_with(common))
        {
            return Ok(true);
        }
        let relative = self.memory_key(path);
        Ok(relative == "manifest.json"
            || relative.starts_with("patch/")
            || manifest.files.contains_key(&relative))
    }

    fn validate_snapshot(&self, manifest: DataManifest) -> Result<DataManifest, LoadError> {
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

    fn with_backends(tag: &str, files: BTreeMap<String, Vec<u8>>, check: impl Fn(&GameData)) {
        check(&GameData::from_memory(files.clone()));
        let root = std::env::temp_dir().join(format!(
            "pobr-gamedata-inventory-{tag}-{}",
            std::process::id()
        ));
        let version = root.join("test");
        for (name, bytes) in files {
            let path = if name.starts_with("overlay-common/") {
                root.join(name)
            } else {
                version.join(name)
            };
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        check(&GameData::new(version));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn strict_inventory_controls_direct_loads_fallbacks_and_tree_enumeration() {
        let mut files = strict_files();
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&files["manifest.json"]).unwrap();
        manifest["files"]["base/passive_trees/listed.json"] =
            format!("{:x}", Sha256::digest(b"[]")).into();
        files.insert(
            "manifest.json".into(),
            serde_json::to_vec(&manifest).unwrap(),
        );
        files.insert("base/passive_trees/listed.json".into(), b"[]".to_vec());
        for name in [
            "base/passive_trees/unlisted.json",
            "base/stats.json",
            "stats.json",
        ] {
            files.insert(name.into(), b"[]".to_vec());
        }
        files.insert(
            "overlay/stat_descriptions.json".into(),
            b"invalid JSON".to_vec(),
        );
        files.insert(
            "i18n/zh-TW/words.json".into(),
            br#"{"hidden":"value"}"#.to_vec(),
        );
        files.insert(
            "overlay/example.json".into(),
            br#"{"value":"unlisted"}"#.to_vec(),
        );
        files.insert(
            "overlay-common/example.json".into(),
            br#"{"value":"common"}"#.to_vec(),
        );
        files.insert(
            "patch/overlay/gem_quality_stats.json".into(),
            br#"{"effects":[{"effect_id":"Patched","stats":[]}]}"#.to_vec(),
        );
        with_backends("closed", files, |data| {
            // Domain calls enforce the inventory without a prior explicit validate.
            assert_eq!(
                data.gem_quality_stats().unwrap().unwrap().effects[0].effect_id,
                "Patched"
            );
            assert!(data.stat_descriptions().unwrap().is_none());
            assert!(data.word_names("zh-TW").unwrap().is_empty());
            assert!(
                matches!(data.stats(), Err(crate::LoadError::Io { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound)
            );
            assert_eq!(data.available_tree_versions(), ["listed"]);
            assert!(data.passive_nodes_versioned("listed").unwrap().is_some());
            assert!(data.passive_nodes_versioned("unlisted").unwrap().is_none());
            let common: serde_json::Value = data.load_overlay_or_common("example.json").unwrap();
            assert_eq!(common["value"], "common");
            assert_eq!(data.validate_manifest().unwrap().schema_version, 3);
        });
    }

    #[test]
    fn direct_optional_loads_cannot_hide_broken_inventory() {
        for missing in [true, false] {
            let mut files = strict_files();
            if missing {
                files.remove("overlay/gem_quality_stats.json");
            } else {
                files.insert("overlay/gem_quality_stats.json".into(), b"{}".to_vec());
            }
            with_backends(if missing { "missing" } else { "changed" }, files, |data| {
                assert!(matches!(
                    data.gem_quality_stats(),
                    Err(crate::LoadError::Integrity { .. })
                ));
                // This loader probes existence before reading, and must still fail.
                assert!(matches!(
                    data.stat_descriptions(),
                    Err(crate::LoadError::Integrity { .. })
                ));
            });
        }
    }

    #[test]
    fn unlisted_quality_is_absent_only_in_strict_snapshots() {
        for schema in [1, 2, 3] {
            let mut files = strict_files();
            let manifest = serde_json::json!({
                "schema_version": schema, "poe_version": "test", "languages": [],
                "domains": { "base": ["stats"] },
                "files": { "base/stats.json": format!("{:x}", Sha256::digest(b"[]")) }
            });
            files.insert(
                "manifest.json".into(),
                serde_json::to_vec(&manifest).unwrap(),
            );
            files.insert("base/stats.json".into(), b"[]".to_vec());
            with_backends(&format!("schema-{schema}"), files, |data| {
                assert_eq!(data.gem_quality_stats().unwrap().is_some(), schema < 3);
                assert!(data.stats().unwrap().is_empty());
            });
        }
    }

    #[test]
    fn standalone_domains_without_manifest_keep_legacy_loading() {
        let mut files = strict_files();
        files.remove("manifest.json");
        with_backends("no-manifest", files, |data| {
            assert!(data.gem_quality_stats().unwrap().is_some());
            assert!(data.validate_manifest().is_err());
        });
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
