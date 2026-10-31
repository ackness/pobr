//! Manifest loading and immutable runtime snapshot validation.
//!
//! Schemas 1/2 preserve legacy domain layouts; schema 3 adds required file hashes.
//! Schema 4 requires an immutable native contract, without calculation admission.
//! The manifest lives at the version root and is never overlaid by user patches.

use pobr_data::catalog::native_player::{NATIVE_ACTOR_BINDING_PATH, NATIVE_PLAYER_CONTEXT_PATH};
use pobr_data::catalog::native_runtime::{
    NATIVE_COVERAGE_PATH, NATIVE_PLAYER_CONTRACT_VERSION, NATIVE_REFERENCE_INDEX_PATH,
    NATIVE_REQUIRED_DOMAINS, NATIVE_RUNTIME_CONTRACT_VERSION, NATIVE_RUNTIME_PATH,
    NativeInputProfile, NativeOutputProfile, NativeProfile,
};
use pobr_data::catalog::{DataManifest, NATIVE_CATALOG_SCHEMA_VERSION, NativeRuntimeContract};
use sha2::{Digest, Sha256};

use crate::{GameData, LoadError};

/// Validated envelope and native metadata share one immutable snapshot cache.
#[derive(Debug)]
pub(crate) struct ValidatedSnapshot {
    pub manifest: DataManifest,
    pub native_runtime: Option<NativeRuntimeContract>,
}

/// Deterministic key for the concrete native assembly declaration.
///
/// All typed metadata participates, including input/compiler/rule pins and profile
/// versions. Set ordering is canonical. The key itself is replaced with an empty
/// string; final runtime inventory hashes belong to `GameData::identity` instead.
pub fn native_assembly_key(contract: &NativeRuntimeContract) -> String {
    let mut inputs = contract.clone();
    inputs.assembly_key.clear();
    let bytes = serde_json::to_vec(&("pobr-native-assembly-v1", inputs))
        .expect("typed native metadata serializes");
    sha256_hex(&bytes)
}

/// Lowercase hex SHA-256 of `bytes`.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

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
    /// Schema 4 additionally validates the mandatory native contract and input profile.
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
        Ok(self.snapshot()?.map(|snapshot| &snapshot.manifest))
    }

    /// Returns the validated, unpatched native contract for schema-4 candidates.
    /// Legacy snapshots have no native contract. Loading this is not calculation admission.
    pub fn native_runtime(&self) -> Result<Option<&NativeRuntimeContract>, LoadError> {
        Ok(self
            .snapshot()?
            .and_then(|snapshot| snapshot.native_runtime.as_ref()))
    }

    /// Rejects ordinary full-build calculation of native catalogs. A bounded native
    /// consumer admits its own exact dataset/save/branch binding and semantics;
    /// this legacy entry must not substitute defaults for unadmitted dependencies.
    pub fn validate_calculation_admission(&self) -> Result<(), LoadError> {
        if self.native_runtime()?.is_some() {
            return Err(LoadError::Integrity {
                path: self.root().join(NATIVE_RUNTIME_PATH),
                message: "native-profile-not-admitted: ordinary full-build constants and rule closure are unavailable; use explicit native admission for a supported bounded evaluation with exact dataset/save binding and stable branch admission".into(),
            });
        }
        Ok(())
    }

    fn snapshot(&self) -> Result<Option<&ValidatedSnapshot>, LoadError> {
        match self.snapshot.get_or_init(|| match self.manifest() {
            Err(LoadError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                match self.native_contract_present() {
                    Ok(false) => Ok(None),
                    Ok(true) => Err("native runtime contract requires manifest schema 4".into()),
                    Err(error) => Err(error.to_string()),
                }
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

    fn native_contract_present(&self) -> Result<bool, LoadError> {
        for relative in [
            NATIVE_RUNTIME_PATH,
            NATIVE_PLAYER_CONTEXT_PATH,
            NATIVE_ACTOR_BINDING_PATH,
        ] {
            let path = self.root().join(relative);
            match self.read_bytes(&path) {
                Ok(_) => return Ok(true),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => return Err(LoadError::Io { path, source }),
            }
        }
        Ok(false)
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
            return Ok(manifest.schema_version < NATIVE_CATALOG_SCHEMA_VERSION);
        }
        let relative = self.memory_key(path);
        Ok(relative == "manifest.json"
            || relative.starts_with("patch/")
            || manifest.files.contains_key(&relative))
    }

    fn validate_snapshot(&self, manifest: DataManifest) -> Result<ValidatedSnapshot, LoadError> {
        let invalid = |message| LoadError::Integrity {
            path: self.root().join("manifest.json"),
            message,
        };
        if !(1..=NATIVE_CATALOG_SCHEMA_VERSION).contains(&manifest.schema_version) {
            return Err(invalid(format!(
                "unsupported schema {}",
                manifest.schema_version
            )));
        }
        if manifest.poe_version.trim().is_empty() {
            return Err(invalid("empty data version".into()));
        }
        if manifest.schema_version < NATIVE_CATALOG_SCHEMA_VERSION {
            // A native policy must never be ignored by a compatibility envelope.
            let present = self.native_contract_present()?;
            if present
                || manifest.files.contains_key(NATIVE_RUNTIME_PATH)
                || manifest
                    .domains
                    .base
                    .iter()
                    .any(|domain| domain == "native_runtime")
            {
                return Err(invalid(
                    "native runtime contract requires manifest schema 4".into(),
                ));
            }
        }
        if manifest.schema_version < 3 {
            return Ok(ValidatedSnapshot {
                manifest,
                native_runtime: None,
            });
        }
        if manifest.files.is_empty() {
            return Err(invalid(
                "schema 3/4 requires a nonempty file inventory".into(),
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
            // The older staging handshake cannot certify schema-4 semantics.
            // Native candidates always verify bytes, even on a prechecked backend.
            if (!self.prechecked || manifest.schema_version == NATIVE_CATALOG_SCHEMA_VERSION)
                && sha256_hex(&bytes) != *digest
            {
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
        let native_runtime = if manifest.schema_version == NATIVE_CATALOG_SCHEMA_VERSION {
            Some(self.validate_native_contract(&manifest)?)
        } else {
            None
        };
        Ok(ValidatedSnapshot {
            manifest,
            native_runtime,
        })
    }

    fn validate_native_contract(
        &self,
        manifest: &DataManifest,
    ) -> Result<NativeRuntimeContract, LoadError> {
        let path = self.root().join(NATIVE_RUNTIME_PATH);
        let invalid = |message| LoadError::Integrity {
            path: path.clone(),
            message,
        };
        for domain in NATIVE_REQUIRED_DOMAINS.iter().copied().chain([
            "native_runtime",
            "native_coverage",
            "native_reference_index",
        ]) {
            if !manifest
                .domains
                .base
                .iter()
                .any(|declared| declared == domain)
                || !manifest.files.contains_key(&format!("base/{domain}.json"))
            {
                return Err(invalid(format!(
                    "native input profile requires base/{domain}.json"
                )));
            }
        }
        // Contract metadata is immutable and cannot be patched around admission checks.
        let patch = self.root().join("patch").join(NATIVE_RUNTIME_PATH);
        match self.read_bytes(&patch) {
            Ok(_) => return Err(invalid("native runtime contract cannot be patched".into())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(LoadError::Io {
                    path: patch,
                    source,
                });
            }
        }
        let bytes = self.read_bytes(&path).map_err(|source| LoadError::Io {
            path: path.clone(),
            source,
        })?;
        let contract: NativeRuntimeContract =
            serde_json::from_slice(&bytes).map_err(|source| LoadError::Parse {
                path: path.clone(),
                source,
            })?;
        if ![
            NATIVE_RUNTIME_CONTRACT_VERSION,
            NATIVE_PLAYER_CONTRACT_VERSION,
        ]
        .contains(&contract.schema_version)
        {
            return Err(invalid(format!(
                "unsupported native contract version {}",
                contract.schema_version
            )));
        }
        // These are complete interpretation tuples, not independently combinable scopes.
        let profile = match contract.input_profile {
            NativeInputProfile::SorceressSparkLevel1Q0V1 => NativeProfile::SparkLevel1Q0,
            NativeInputProfile::SorceressSparkLevel1Q0OrdinaryDamagePassivesV1
                if contract.schema_version == NATIVE_PLAYER_CONTRACT_VERSION =>
            {
                NativeProfile::SparkLevel1Q0OrdinaryDamagePassives
            }
            NativeInputProfile::SelectedPlayerPrimarySpellHitQ0PassivesV1
                if contract.schema_version == NATIVE_PLAYER_CONTRACT_VERSION =>
            {
                NativeProfile::PrimarySpellHitQ0Passives
            }
            _ => return Err(invalid("unsupported native input/profile tuple".into())),
        };
        if contract.normalization.profile != profile || contract.rules.profile != profile {
            return Err(invalid("mixed native input/profile tuple".into()));
        }
        let player_paths = [NATIVE_PLAYER_CONTEXT_PATH, NATIVE_ACTOR_BINDING_PATH];
        if contract.schema_version == NATIVE_RUNTIME_CONTRACT_VERSION {
            if contract.player.is_some()
                || contract.output_profile != NativeOutputProfile::SparkSingleHitCastCostV1
            {
                return Err(invalid(
                    "v2 player artifacts cannot appear under native contract v1".into(),
                ));
            }
            for relative in player_paths {
                let path = self.root().join(relative);
                match self.read_bytes(&path) {
                    Ok(_) => {
                        return Err(invalid(
                            "v2 player artifacts cannot appear under native contract v1".into(),
                        ));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(source) => return Err(LoadError::Io { path, source }),
                }
                if manifest.files.contains_key(relative)
                    || manifest
                        .domains
                        .base
                        .iter()
                        .any(|d| relative == format!("base/{d}.json"))
                {
                    return Err(invalid(
                        "v2 player artifacts cannot appear under native contract v1".into(),
                    ));
                }
            }
        } else {
            let player = contract
                .player
                .as_ref()
                .ok_or_else(|| invalid("native contract v2 requires player artifacts".into()))?;
            let output = if profile == NativeProfile::PrimarySpellHitQ0Passives {
                NativeOutputProfile::ConditionalSkillHitV2
            } else {
                NativeOutputProfile::ConditionalSkillHitV1
            };
            if contract.output_profile != output {
                return Err(invalid(
                    "native input requires its exact conditional-skill-hit output profile".into(),
                ));
            }
            for (relative, digest) in [
                (NATIVE_PLAYER_CONTEXT_PATH, &player.context_sha256),
                (NATIVE_ACTOR_BINDING_PATH, &player.actor_binding_sha256),
            ] {
                if manifest.files.get(relative) != Some(digest)
                    || !manifest
                        .domains
                        .base
                        .iter()
                        .any(|d| relative == format!("base/{d}.json"))
                {
                    return Err(invalid(format!(
                        "native v2 artifact missing or digest disagrees with inventory: {relative}"
                    )));
                }
            }
            if player.parent_manifest_sha256 != contract.tree.source_manifest_sha256
                || !contract
                    .allowed_source_manifests
                    .contains(&player.source_manifest_sha256)
                || !contract
                    .allowed_source_manifests
                    .contains(&player.parent_manifest_sha256)
            {
                return Err(invalid(
                    "native player and exact parent source manifests must be explicitly allowed"
                        .into(),
                ));
            }
        }
        if contract.game_patch != manifest.poe_version
            || contract.game_patch.split('.').count() < 2
            || !contract
                .game_patch
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|c| c.is_ascii_digit()))
        {
            return Err(invalid(
                "native game patch must be an explicit dotted patch matching poe_version".into(),
            ));
        }
        if contract.tree.compatibility_tag != "0_5" {
            return Err(invalid(
                "unsupported native tree compatibility tag (expected 0_5)".into(),
            ));
        }
        let resource = &contract.tree.resource;
        if resource.contains(['\\', ':'])
            || resource
                .split('/')
                .any(|part| matches!(part, "" | "." | ".."))
        {
            return Err(invalid(
                "native tree resource must be a canonical relative address".into(),
            ));
        }
        if contract.allowed_source_manifests.is_empty()
            || !contract
                .allowed_source_manifests
                .contains(&contract.tree.source_manifest_sha256)
        {
            return Err(invalid(
                "native tree source manifest must be explicitly allowed".into(),
            ));
        }
        for profile in [&contract.normalization, &contract.rules] {
            if profile.version != contract.schema_version {
                return Err(invalid(format!(
                    "unsupported native profile version {}",
                    profile.version
                )));
            }
        }
        let mut digests = vec![
            contract.assembly_key.as_str(),
            contract.source_lock_sha256.as_str(),
            contract.descriptions_manifest_sha256.as_str(),
            contract.tree.source_manifest_sha256.as_str(),
            contract.tree.resource_sha256.as_str(),
            contract.normalization.sha256.as_str(),
            contract.rules.sha256.as_str(),
            contract.coverage_sha256.as_str(),
            contract.reference_index_sha256.as_str(),
            contract.acquisition.evidence_sha256.as_str(),
        ];
        if let Some(player) = &contract.player {
            digests.extend([
                player.context_sha256.as_str(),
                player.actor_binding_sha256.as_str(),
                player.source_manifest_sha256.as_str(),
                player.source_lock_sha256.as_str(),
                player.parent_manifest_sha256.as_str(),
                player.parent_source_lock_sha256.as_str(),
            ]);
        }
        digests.extend(contract.allowed_source_manifests.iter().map(String::as_str));
        for supplement in &contract.supplements {
            if supplement.fields.is_empty()
                || supplement
                    .game_patch
                    .as_ref()
                    .is_some_and(|patch| patch != &contract.game_patch)
            {
                return Err(invalid(
                    "native supplement requires explicit fields and a matching or unknown patch"
                        .into(),
                ));
            }
            digests.extend([
                supplement.sha256.as_str(),
                supplement.match_evidence_sha256.as_str(),
            ]);
        }
        if digests.iter().any(|digest| !is_lower_hex(digest, 64))
            || !is_lower_hex(&contract.compiler_revision, 40)
        {
            return Err(invalid(
                "invalid native SHA-256 or compiler revision".into(),
            ));
        }
        for (file, digest) in [
            (NATIVE_COVERAGE_PATH, &contract.coverage_sha256),
            (
                NATIVE_REFERENCE_INDEX_PATH,
                &contract.reference_index_sha256,
            ),
        ] {
            if manifest.files.get(file) != Some(digest) {
                return Err(invalid(format!(
                    "native artifact digest disagrees with inventory: {file}"
                )));
            }
        }
        if contract.schema_version == NATIVE_PLAYER_CONTRACT_VERSION {
            // Every v2 interpretation/provenance sidecar is sealed, including rules.
            for (relative, digest) in [
                ("base/native_input_lock.json", &contract.source_lock_sha256),
                (
                    "base/native_normalization.json",
                    &contract.normalization.sha256,
                ),
                ("base/native_rule_requirements.json", &contract.rules.sha256),
                (
                    "base/native_acquisition.json",
                    &contract.acquisition.evidence_sha256,
                ),
            ] {
                if manifest.files.get(relative) != Some(digest) {
                    return Err(invalid(format!(
                        "native v2 artifact digest disagrees with inventory: {relative}"
                    )));
                }
            }
            for supplement in &contract.supplements {
                if !manifest
                    .files
                    .values()
                    .any(|sha| sha == &supplement.match_evidence_sha256)
                {
                    return Err(invalid(
                        "native v2 tree match evidence must be sealed".into(),
                    ));
                }
            }
            crate::native::validate_player(self, manifest, &contract)?;
        }
        if native_assembly_key(&contract) != contract.assembly_key {
            return Err(invalid("native assembly key mismatch".into()));
        }
        Ok(contract)
    }
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::sha256_hex;
    use crate::GameData;
    use std::collections::BTreeMap;

    fn strict_files() -> BTreeMap<String, Vec<u8>> {
        let quality = br#"{"effects":[]}"#.to_vec();
        let manifest = serde_json::json!({
            "schema_version": 3, "poe_version": "test", "languages": [],
            "domains": { "base": [], "overlay": ["gem_quality_stats"], "generated": [] },
            "files": { "overlay/gem_quality_stats.json": sha256_hex(&quality) }
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

    /// A pack the caller says it has checked is still structurally validated, and
    /// keeps the identity the unchecked loader gives it; only the byte comparison
    /// is left out.
    #[test]
    fn a_prechecked_pack_skips_only_the_digest_comparison() {
        let files = strict_files();
        let checked = GameData::from_memory_prechecked(files.clone());
        checked.validate_manifest().unwrap();
        assert_eq!(
            checked.identity().unwrap(),
            GameData::from_memory(files.clone()).identity().unwrap()
        );
        // A changed file is the caller's to catch; the loader no longer hashes it.
        let mut changed = files.clone();
        changed.insert("overlay/gem_quality_stats.json".into(), b"{}".to_vec());
        GameData::from_memory_prechecked(changed)
            .validate_manifest()
            .unwrap();
        // A listed file that is missing, and a malformed inventory, still fail.
        let mut missing = files.clone();
        missing.remove("overlay/gem_quality_stats.json");
        assert!(
            GameData::from_memory_prechecked(missing)
                .validate_manifest()
                .is_err()
        );
        let mut malformed: serde_json::Value =
            serde_json::from_slice(&files["manifest.json"]).unwrap();
        malformed["files"]["overlay/gem_quality_stats.json"] = "NOT-A-DIGEST".into();
        let mut bad_inventory = files;
        bad_inventory.insert(
            "manifest.json".into(),
            serde_json::to_vec(&malformed).unwrap(),
        );
        assert!(
            GameData::from_memory_prechecked(bad_inventory)
                .validate_manifest()
                .is_err()
        );
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
        manifest["files"]["base/passive_trees/listed.json"] = sha256_hex(b"[]").into();
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
                "files": { "base/stats.json": sha256_hex(b"[]") }
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
