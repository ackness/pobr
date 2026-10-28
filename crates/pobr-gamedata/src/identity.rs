//! Stable identity of the runtime input pack, including separate curation and patches.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{GameData, LoadError, manifest::sha256_hex};

/// Identity of the data pack available to a loader, independent of its disk location.
///
/// This identifies input bytes, not numerical equivalence: whitespace, translations,
/// or an overridden input can change the digest without changing calculation outputs.
/// Compiled-in rules and defaults belong to the engine identity, not this data identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataIdentity {
    /// Manifest version; absent only for standalone legacy data without a manifest.
    pub data_version: Option<String>,
    /// Lowercase SHA-256 of the versioned input-pack fingerprint representation.
    pub sha256: String,
}

// Keep legacy packs consistent with the runtime inventory published by
// pipeline/data_snapshot.py. Schema 3 uses its explicit inventory instead.
const MAINTENANCE_FILES: &[&str] = &[
    "generated/modifier-audit.json",
    "generated/parsed_mods.json",
    "generated/parse-coverage.json",
    "generated/test_pins.json",
    "generated/translation-audit.json",
    "overlay/stat_id_map.json",
];

impl GameData {
    /// Identifies this immutable runtime input pack once; clones reuse the result.
    ///
    /// Schema 3 uses validated inventory hashes, ignoring unlisted version files.
    /// Legacy schemas (including no manifest) hash present JSON files, excluding
    /// maintenance reports; missing optional files contribute no entry. Shared
    /// `overlay-common` JSON files and patches of present snapshot files are added
    /// separately. Orphan patches and patches of the manifest/common layer do not
    /// affect runtime loading and are excluded. An unreadable input is an error.
    ///
    /// The digest covers sorted canonical relative paths and byte hashes plus
    /// schema/version metadata. It ignores absolute roots, manifest formatting,
    /// declaration ordering and directory iteration order. Memory keys must use
    /// canonical forward-slash paths; ambiguous aliases are rejected. Directory
    /// symlinks are rejected when discovering legacy/shared files.
    ///
    /// As with manifest validation, both success and failure are cached. Obtain
    /// this alongside loading build data; after changing files, create a new
    /// loader. This is a pack identity, not a digest of just the domains a specific
    /// build happens to read, and does not make mutable disk inputs safe.
    pub fn identity(&self) -> Result<&DataIdentity, LoadError> {
        match self
            .identity
            .get_or_init(|| self.compute_identity().map_err(|error| error.to_string()))
        {
            Ok(identity) => Ok(identity),
            Err(message) => Err(LoadError::Integrity {
                path: self.root().to_owned(),
                message: message.clone(),
            }),
        }
    }

    fn compute_identity(&self) -> Result<DataIdentity, LoadError> {
        let manifest = self.snapshot_manifest()?;
        let mut hashes = match manifest {
            Some(manifest) if manifest.schema_version >= 3 => manifest.files.clone(),
            _ => {
                let mut hashes = BTreeMap::new();
                for relative in self.json_files(self.root(), "")? {
                    if relative == "manifest.json"
                        || relative.starts_with("patch/")
                        || relative.starts_with("overlay-common/")
                        || MAINTENANCE_FILES.contains(&relative.as_str())
                    {
                        continue;
                    }
                    self.hash_file(&mut hashes, relative.clone(), self.root().join(relative))?;
                }
                hashes
            }
        };
        // A patch is only read after its underlying snapshot file has loaded.
        // Never include orphan patches or patch/manifest.json.
        for relative in hashes.keys().cloned().collect::<Vec<_>>() {
            let patch = format!("patch/{relative}");
            let path = self.root().join(&patch);
            match self.read_bytes(&path) {
                Ok(bytes) => {
                    hashes.insert(patch, sha256_hex(&bytes));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => return Err(LoadError::Io { path, source }),
            }
        }
        if let Some(common) = self.overlay_common_path("") {
            for relative in self.json_files(&common, "overlay-common/")? {
                self.hash_file(
                    &mut hashes,
                    format!("overlay-common/{relative}"),
                    common.join(relative),
                )?;
            }
        }
        let data_version = manifest.map(|manifest| manifest.poe_version.clone());
        // A format tag keeps future changes to fingerprint semantics distinguishable.
        // BTreeMap plus length-delimited JSON strings makes ordering and boundaries
        // unambiguous without depending on manifest's original JSON serialization.
        let fingerprint = serde_json::to_vec(&(
            "pobr-data-pack-v1",
            manifest.map(|manifest| manifest.schema_version),
            &data_version,
            hashes,
        ))
        .expect("strings and integers serialize");
        Ok(DataIdentity {
            data_version,
            sha256: sha256_hex(&fingerprint),
        })
    }

    fn hash_file(
        &self,
        hashes: &mut BTreeMap<String, String>,
        relative: String,
        path: PathBuf,
    ) -> Result<(), LoadError> {
        let bytes = self
            .read_bytes(&path)
            .map_err(|source| LoadError::Io { path, source })?;
        hashes.insert(relative, sha256_hex(&bytes));
        Ok(())
    }

    /// Enumerates JSON paths relative to a layer. Memory's common layer lives at
    /// the explicit `overlay-common/` prefix; disk's lives beside the version root.
    fn json_files(&self, directory: &Path, memory_prefix: &str) -> Result<Vec<String>, LoadError> {
        if let Some(files) = &self.files {
            return files
                .keys()
                .iter()
                .filter_map(|key| key.strip_prefix(memory_prefix))
                .filter(|key| key.ends_with(".json"))
                .map(|key| {
                    validate_relative(key, directory)?;
                    Ok(key.to_owned())
                })
                .collect();
        }
        let mut found = Vec::new();
        collect_json_files(directory, directory, &mut found)?;
        Ok(found)
    }
}

fn validate_relative(relative: &str, directory: &Path) -> Result<(), LoadError> {
    if relative.contains(['\\', ':'])
        || relative
            .split('/')
            .any(|part| matches!(part, "" | "." | ".."))
    {
        return Err(LoadError::Integrity {
            path: directory.to_owned(),
            message: format!("noncanonical data identity path: {relative}"),
        });
    }
    Ok(())
}

fn collect_json_files(
    root: &Path,
    directory: &Path,
    found: &mut Vec<String>,
) -> Result<(), LoadError> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(LoadError::Io {
                path: directory.to_owned(),
                source,
            });
        }
    };
    for entry in entries {
        let entry = entry.map_err(|source| LoadError::Io {
            path: directory.to_owned(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| LoadError::Io {
            path: path.clone(),
            source,
        })?;
        if file_type.is_symlink() && path.is_dir() {
            return Err(LoadError::Integrity {
                path,
                message: "directory symlinks cannot be fingerprinted".into(),
            });
        }
        if file_type.is_dir() {
            collect_json_files(root, &path, found)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            let relative = path
                .strip_prefix(root)
                .expect("enumerated path is inside layer root")
                .to_str()
                .ok_or_else(|| LoadError::Integrity {
                    path: path.clone(),
                    message: "data identity paths must be UTF-8".into(),
                })?
                .replace(std::path::MAIN_SEPARATOR, "/");
            validate_relative(&relative, root)?;
            found.push(relative);
        }
    }
    Ok(())
}
