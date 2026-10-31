//! Guarded native metadata is loadable, but cannot certify a calculation yet.

use std::collections::BTreeMap;
use std::path::PathBuf;

use pobr_data::catalog::NativeRuntimeContract;
use pobr_data::catalog::native_runtime::{NATIVE_REQUIRED_DOMAINS, NATIVE_RUNTIME_PATH};
use pobr_gamedata::{GameData, LoadError, manifest::native_assembly_key};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn pack() -> BTreeMap<String, Vec<u8>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/native_runtime");
    let bytes = std::fs::read(root.join("manifest.json")).unwrap();
    let manifest: Value = serde_json::from_slice(&bytes).unwrap();
    let mut files: BTreeMap<_, _> = manifest["files"]
        .as_object()
        .unwrap()
        .keys()
        .map(|file| (file.clone(), std::fs::read(root.join(file)).unwrap()))
        .collect();
    files.insert("manifest.json".into(), bytes);
    files
}

fn edit(files: &mut BTreeMap<String, Vec<u8>>, path: &str, change: impl FnOnce(&mut Value)) {
    let mut value = serde_json::from_slice(&files[path]).unwrap();
    change(&mut value);
    files.insert(path.into(), serde_json::to_vec(&value).unwrap());
}

fn reseal(files: &mut BTreeMap<String, Vec<u8>>) {
    let contract: NativeRuntimeContract =
        serde_json::from_slice(&files[NATIVE_RUNTIME_PATH]).unwrap();
    edit(files, NATIVE_RUNTIME_PATH, |v| {
        v["assembly_key"] = native_assembly_key(&contract).into()
    });
    seal_inventory(files);
}

fn seal_inventory(files: &mut BTreeMap<String, Vec<u8>>) {
    let hashes: BTreeMap<_, _> = files
        .iter()
        .filter(|(p, _)| p.starts_with("base/"))
        .map(|(p, b)| (p.clone(), hash(b)))
        .collect();
    edit(files, "manifest.json", |v| {
        v["files"] = serde_json::to_value(hashes).unwrap()
    });
}

struct DiskPack(PathBuf);
impl DiskPack {
    fn new(files: &BTreeMap<String, Vec<u8>>) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "pobr-native-contract-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let pack = Self(root);
        for (relative, bytes) in files.iter().rev() {
            let path = if relative.starts_with("overlay-common/") {
                pack.0.join(relative)
            } else {
                pack.version().join(relative)
            };
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        pack
    }
    fn version(&self) -> PathBuf {
        self.0.join("version")
    }
}
impl Drop for DiskPack {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn rejected(files: BTreeMap<String, Vec<u8>>, message: &str) {
    let disk = DiskPack::new(&files);
    for data in [
        GameData::from_memory(files.clone()),
        GameData::from_memory_prechecked(files),
        GameData::new(disk.version()),
    ] {
        let error = data.validate_manifest().unwrap_err().to_string();
        assert!(error.contains(message), "expected {message:?}, got {error}");
        assert!(data.native_runtime().is_err());
        assert!(data.identity().is_err());
        assert!(data.load_ruleset().is_err());
    }
}

#[test]
fn native_contract_disk_memory_identity_and_query_agree() {
    let files = pack();
    let disk = DiskPack::new(&files);
    let memory = GameData::from_memory(files.clone());
    for data in [
        GameData::new(disk.version()),
        GameData::from_memory_prechecked(files.clone()),
        GameData::from_memory_releasing(files, true),
    ] {
        assert_eq!(data.identity().unwrap(), memory.identity().unwrap());
        assert_eq!(
            data.native_runtime().unwrap(),
            memory.native_runtime().unwrap()
        );
        assert_eq!(
            data.native_runtime()
                .unwrap()
                .unwrap()
                .tree
                .compatibility_tag,
            "0_5"
        );
        assert!(data.passive_nodes().unwrap().is_empty());
        assert!(data.skill_gems().unwrap().is_empty());
        data.release_domain("passive_tree.json");
        assert_eq!(data.identity().unwrap(), memory.identity().unwrap());
        assert_eq!(
            data.native_runtime().unwrap(),
            memory.native_runtime().unwrap()
        );
        assert!(
            matches!(data.load_ruleset(), Err(LoadError::Integrity { message, .. }) if message.starts_with("native-profile-not-admitted:"))
        );
    }
}

#[test]
fn native_reserved_metadata_requires_envelope_even_without_manifest() {
    for malformed in [false, true] {
        let mut files = pack();
        files.remove("manifest.json");
        if malformed {
            files.insert(
                NATIVE_RUNTIME_PATH.into(),
                b"malformed native policy".to_vec(),
            );
        }
        files.insert(
            "overlay-common/special_mods.json".into(),
            br#"{"entries":[]}"#.to_vec(),
        );
        let disk = DiskPack::new(&files);
        for data in [
            GameData::new(disk.version()),
            GameData::from_memory(files.clone()),
            GameData::from_memory_prechecked(files.clone()),
            GameData::from_memory_releasing(files.clone(), false),
            GameData::from_memory_releasing(files, true),
        ] {
            assert!(data.validate_manifest().is_err());
            let error = data.native_runtime().unwrap_err().to_string();
            assert!(
                error.contains("native runtime contract requires manifest schema 4"),
                "{error}"
            );
            assert!(data.validate_calculation_admission().is_err());
            assert!(
                data.load_ruleset().is_err(),
                "legacy rules loaded native policy"
            );
            assert!(data.identity().is_err());
        }
    }
    // An unmarked manifestless pack retains its historical ruleset behavior.
    let files = BTreeMap::from([(
        "overlay-common/special_mods.json".into(),
        br#"{"entries":[]}"#.to_vec(),
    )]);
    let disk = DiskPack::new(&files);
    for data in [
        GameData::new(disk.version()),
        GameData::from_memory(files.clone()),
        GameData::from_memory_prechecked(files.clone()),
        GameData::from_memory_releasing(files.clone(), false),
        GameData::from_memory_releasing(files, true),
    ] {
        assert!(data.native_runtime().unwrap().is_none());
        assert!(data.validate_calculation_admission().is_ok());
        assert!(data.load_ruleset().unwrap().special_mods.is_some());
    }
}

#[test]
fn native_unreadable_reserved_metadata_cannot_become_legacy_absence() {
    let files = BTreeMap::from([(NATIVE_RUNTIME_PATH.into(), b"reserved".to_vec())]);
    let disk = DiskPack::new(&files);
    let path = disk.version().join(NATIVE_RUNTIME_PATH);
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    let data = GameData::new(disk.version());
    assert!(data.native_runtime().is_err());
    assert!(data.validate_calculation_admission().is_err());
    assert!(data.load_ruleset().is_err());
}

#[test]
fn native_contract_and_required_files_cannot_be_missing_or_unlisted() {
    for domain in NATIVE_REQUIRED_DOMAINS.iter().copied().chain([
        "native_runtime",
        "native_coverage",
        "native_reference_index",
    ]) {
        let path = format!("base/{domain}.json");
        let mut files = pack();
        files.remove(&path);
        rejected(files, &path);
        let mut files = pack();
        edit(&mut files, "manifest.json", |m| {
            m["files"].as_object_mut().unwrap().remove(&path);
            m["domains"]["base"]
                .as_array_mut()
                .unwrap()
                .retain(|v| v != domain);
        });
        rejected(files, "native input profile requires");
    }
}

#[test]
fn native_contract_rejects_versions_profiles_and_bad_declarations() {
    for (field, value, expected) in [
        (
            "schema_version",
            json!(99),
            "unsupported native contract version",
        ),
        ("game", json!("poe1"), "unknown variant"),
        ("realm", json!("CN"), "unknown variant"),
        ("game_patch", json!("0_5"), "native game patch"),
        ("game_patch", json!("0.5.5.6"), "native game patch"),
        ("input_profile", json!("all-builds-v1"), "unknown variant"),
        ("output_profile", json!("all-fields-v1"), "unknown variant"),
        ("source_lock_sha256", json!("bad"), "invalid native SHA-256"),
        (
            "compiler_revision",
            json!("latest"),
            "invalid native SHA-256",
        ),
        (
            "allowed_source_manifests",
            json!([]),
            "must be explicitly allowed",
        ),
    ] {
        let mut files = pack();
        edit(&mut files, NATIVE_RUNTIME_PATH, |v| v[field] = value);
        seal_inventory(&mut files);
        rejected(files, expected);
    }
    for profile in ["normalization", "rules"] {
        let mut files = pack();
        edit(&mut files, NATIVE_RUNTIME_PATH, |v| {
            v[profile]["version"] = 2.into()
        });
        seal_inventory(&mut files);
        rejected(files, "unsupported native profile version");
        let mut files = pack();
        edit(&mut files, NATIVE_RUNTIME_PATH, |v| {
            v[profile]["profile"] = "unknown".into()
        });
        seal_inventory(&mut files);
        rejected(files, "unknown variant");
    }
    let original: Value = serde_json::from_slice(&pack()[NATIVE_RUNTIME_PATH]).unwrap();
    for field in original.as_object().unwrap().keys() {
        let mut files = pack();
        edit(&mut files, NATIVE_RUNTIME_PATH, |v| {
            v.as_object_mut().unwrap().remove(field);
        });
        seal_inventory(&mut files);
        rejected(files, "missing field");
    }
    let mut files = pack();
    files.insert(NATIVE_RUNTIME_PATH.into(), b"not JSON".to_vec());
    seal_inventory(&mut files);
    rejected(files, "failed to parse");
    let mut files = pack();
    edit(&mut files, NATIVE_RUNTIME_PATH, |v| {
        v["optional_policy"] = json!({})
    });
    seal_inventory(&mut files);
    rejected(files, "unknown field");
}

#[test]
fn native_contract_rejects_tree_and_supplement_ambiguity() {
    for (field, value, expected) in [
        ("compatibility_tag", "0_4", "tree compatibility tag"),
        (
            "source_manifest_sha256",
            &"f".repeat(64),
            "must be explicitly allowed",
        ),
        ("resource", "../Default.psg", "canonical relative address"),
    ] {
        let mut files = pack();
        edit(&mut files, NATIVE_RUNTIME_PATH, |v| {
            v["tree"][field] = value.into()
        });
        seal_inventory(&mut files);
        rejected(files, expected);
    }
    let supplement = json!({"sha256": "a".repeat(64), "game_patch": null,
        "fields": ["coordinates"], "match_evidence_sha256": "b".repeat(64)});
    let mut files = pack();
    edit(&mut files, NATIVE_RUNTIME_PATH, |v| {
        v["supplements"] = json!([supplement.clone()])
    });
    reseal(&mut files);
    GameData::from_memory(files).validate_manifest().unwrap();
    for (field, value, expected) in [
        ("fields", json!([]), "requires explicit fields"),
        ("fields", json!(["numeric_stats"]), "unknown variant"),
        (
            "game_patch",
            json!("0.5.5.4.2"),
            "matching or unknown patch",
        ),
    ] {
        let mut files = pack();
        let mut bad = supplement.clone();
        bad[field] = value;
        edit(&mut files, NATIVE_RUNTIME_PATH, |v| {
            v["supplements"] = json!([bad])
        });
        seal_inventory(&mut files);
        rejected(files, expected);
    }
    let mut files = pack();
    let mut implicit_unknown_patch = supplement;
    implicit_unknown_patch
        .as_object_mut()
        .unwrap()
        .remove("game_patch");
    edit(&mut files, NATIVE_RUNTIME_PATH, |v| {
        v["supplements"] = json!([implicit_unknown_patch])
    });
    seal_inventory(&mut files);
    rejected(files, "missing field `game_patch`");
}

#[test]
fn native_contract_tampering_requires_new_assembly_and_runtime_identity() {
    let original = pack();
    let initial = GameData::from_memory(original.clone())
        .identity()
        .unwrap()
        .clone();
    let mut changed = original.clone();
    edit(&mut changed, NATIVE_RUNTIME_PATH, |v| {
        v["source_lock_sha256"] = "a".repeat(64).into()
    });
    rejected(changed.clone(), "SHA-256 mismatch");
    seal_inventory(&mut changed);
    rejected(changed.clone(), "assembly key mismatch");
    reseal(&mut changed);
    let disk = DiskPack::new(&changed);
    let updated = GameData::from_memory(changed).identity().unwrap().clone();
    assert_ne!(updated, initial);
    assert_eq!(GameData::new(disk.version()).identity().unwrap(), &updated);
    for artifact in ["native_coverage", "native_reference_index"] {
        let mut changed = original.clone();
        edit(&mut changed, &format!("base/{artifact}.json"), |v| {
            v["scope"] = "changed".into()
        });
        seal_inventory(&mut changed);
        rejected(changed, "native artifact digest disagrees");
    }
    let mut patched = original;
    patched.insert("patch/base/native_runtime.json".into(), b"{}".to_vec());
    rejected(patched, "cannot be patched");
}

#[test]
fn native_contract_interpretation_and_evidence_pins_affect_identity() {
    let original = pack();
    let initial_data = GameData::from_memory(original.clone());
    let initial_contract = initial_data.native_runtime().unwrap().unwrap();
    let initial_identity = initial_data.identity().unwrap();
    for pointer in [
        "/compiler_revision",
        "/descriptions_manifest_sha256",
        "/tree/resource_sha256",
        "/normalization/sha256",
        "/rules/sha256",
        "/acquisition/evidence_sha256",
    ] {
        let mut files = original.clone();
        edit(&mut files, NATIVE_RUNTIME_PATH, |v| {
            *v.pointer_mut(pointer).unwrap() = "a"
                .repeat(if pointer == "/compiler_revision" {
                    40
                } else {
                    64
                })
                .into();
        });
        reseal(&mut files);
        let data = GameData::from_memory(files);
        assert_ne!(data.identity().unwrap(), initial_identity, "{pointer}");
        assert_ne!(
            data.native_runtime().unwrap().unwrap().assembly_key,
            initial_contract.assembly_key,
            "{pointer}"
        );
    }
    // Byte identity is stricter than the typed assembly declaration.
    let mut formatted = original;
    formatted.get_mut(NATIVE_RUNTIME_PATH).unwrap().push(b'\n');
    seal_inventory(&mut formatted);
    let data = GameData::from_memory(formatted);
    assert_eq!(data.native_runtime().unwrap().unwrap(), initial_contract);
    assert_ne!(data.identity().unwrap(), initial_identity);
}

#[test]
fn native_contract_schema_boundary_preserves_legacy_behavior() {
    for schema in [1, 2, 3] {
        let mut files = pack();
        edit(&mut files, "manifest.json", |m| {
            m["schema_version"] = schema.into()
        });
        rejected(files, "requires manifest schema 4");
    }
    for schema in [0, 5, u32::MAX] {
        let mut files = pack();
        edit(&mut files, "manifest.json", |m| {
            m["schema_version"] = schema.into()
        });
        rejected(files, "unsupported schema");
    }
    let mut legacy = pack();
    legacy.remove(NATIVE_RUNTIME_PATH);
    edit(&mut legacy, "manifest.json", |m| {
        m["schema_version"] = 3.into();
        m["files"]
            .as_object_mut()
            .unwrap()
            .remove(NATIVE_RUNTIME_PATH);
        m["domains"]["base"]
            .as_array_mut()
            .unwrap()
            .retain(|v| v != "native_runtime");
    });
    let data = GameData::from_memory(legacy);
    assert!(data.native_runtime().unwrap().is_none());
    assert!(data.load_ruleset().unwrap().game_constants.is_none());
    data.validate_calculation_admission().unwrap();
}

#[test]
fn native_contract_does_not_inherit_ambient_common() {
    let mut files = pack();
    let expected = GameData::from_memory(files.clone())
        .identity()
        .unwrap()
        .clone();
    // Invalid ambient JSON would fail if the common layer leaked into this candidate.
    files.insert(
        "overlay-common/special_mods.json".into(),
        b"invalid ambient JSON".to_vec(),
    );
    files.insert(
        "overlay-common/buff_definitions.json".into(),
        b"invalid ambient JSON".to_vec(),
    );
    let disk = DiskPack::new(&files);
    for data in [GameData::from_memory(files), GameData::new(disk.version())] {
        assert!(data.special_mods().unwrap().is_none());
        assert!(data.buff_definitions().unwrap().is_none());
        assert_eq!(data.identity().unwrap(), &expected);
    }
}

fn player_pack() -> BTreeMap<String, Vec<u8>> {
    let fixture: Value = serde_json::from_slice(include_bytes!(
        "../../../../tools/pobr-data-adapter/tests/fixtures/native-runtime/player-synthetic.json"
    ))
    .unwrap();
    let mut files = pack();
    let context = serde_json::to_vec(&fixture["context"]).unwrap();
    let context_sha = hash(&context);
    let mut binding = fixture["binding"].clone();
    binding["context_sha256"] = context_sha.clone().into();
    let binding = serde_json::to_vec(&binding).unwrap();
    let binding_sha = hash(&binding);
    files.insert("base/native_player_context.json".into(), context);
    files.insert("base/native_actor_binding.json".into(), binding);
    for name in [
        "native_input_lock",
        "native_normalization",
        "native_rule_requirements",
        "native_acquisition",
    ] {
        files.insert(format!("base/{name}.json"), b"{}".to_vec());
    }
    edit(&mut files, NATIVE_RUNTIME_PATH, |v| {
        v["schema_version"] = 2.into();
        v["normalization"]["version"] = 2.into();
        v["normalization"]["sha256"] = hash(b"{}").into();
        v["rules"]["version"] = 2.into();
        v["rules"]["sha256"] = hash(b"{}").into();
        v["source_lock_sha256"] = hash(b"{}").into();
        v["acquisition"]["evidence_sha256"] = hash(b"{}").into();
        v["output_profile"] = "conditional-skill-hit-v1".into();
        v["tree"]["source_manifest_sha256"] = fixture["context"]["parent_manifest_sha256"].clone();
        v["allowed_source_manifests"] = json!([
            fixture["context"]["source_manifest_sha256"],
            fixture["context"]["parent_manifest_sha256"]
        ]);
        v["player"] = json!({"context_sha256":context_sha,"actor_binding_sha256":binding_sha,
            "source_manifest_sha256":fixture["context"]["source_manifest_sha256"],
            "source_lock_sha256":fixture["context"]["source_lock_sha256"],
            "parent_manifest_sha256":fixture["context"]["parent_manifest_sha256"],
            "parent_source_lock_sha256":fixture["context"]["parent_source_lock_sha256"]});
    });
    edit(&mut files, "manifest.json", |v| {
        for name in [
            "native_player_context",
            "native_actor_binding",
            "native_input_lock",
            "native_normalization",
            "native_rule_requirements",
            "native_acquisition",
        ] {
            v["domains"]["base"]
                .as_array_mut()
                .unwrap()
                .push(name.into());
        }
    });
    reseal(&mut files);
    files
}

#[test]
fn native_data_v1_has_no_actor_defaults_and_preserves_contract_serialization() {
    let files = pack();
    let original: Value = serde_json::from_slice(&files[NATIVE_RUNTIME_PATH]).unwrap();
    let contract: NativeRuntimeContract = serde_json::from_value(original.clone()).unwrap();
    assert!(contract.player.is_none());
    assert_eq!(serde_json::to_value(&contract).unwrap(), original);
    assert_eq!(native_assembly_key(&contract), contract.assembly_key);
    let data = GameData::from_memory(files);
    let native = pobr_gamedata::NativeData::load(&data).unwrap();
    assert!(native.player_context().is_none());
    assert!(native.actor_binding().is_none());
    assert_eq!(native.identity(), data.identity().unwrap());
    assert!(data.load_ruleset().is_err());
    assert!(data.validate_calculation_admission().is_err());
}

#[test]
fn native_data_v2_disk_memory_exact_identity_and_actor_policy_agree() {
    let files = player_pack();
    let disk = DiskPack::new(&files);
    let memory = GameData::from_memory(files.clone());
    let expected = pobr_gamedata::NativeData::load(&memory).unwrap();
    for data in [
        GameData::new(disk.version()),
        GameData::from_memory_prechecked(files.clone()),
        GameData::from_memory_releasing(files, true),
    ] {
        let native = pobr_gamedata::NativeData::load(&data).unwrap();
        assert_eq!(native.identity(), expected.identity());
        assert_ne!(native.identity().sha256, native.contract().assembly_key);
        assert_eq!(native.player_context(), expected.player_context());
        assert_eq!(native.actor_binding(), expected.actor_binding());
        let context = native.player_context().unwrap();
        assert_eq!(context.facts.character.source.row, 7);
        assert!(context.facts.selected_class_overrides.is_empty());
        assert_eq!(
            native.actor_binding().unwrap().critical_bonus.bonus_percent,
            37.5
        );
        assert!(native.passive_nodes().is_empty());
        assert!(native.skill_gems().is_empty());
        assert!(data.load_ruleset().is_err());
        assert!(data.validate_calculation_admission().is_err());
    }
}

#[test]
fn native_v2_artifacts_required_and_v1_smuggling_rejected() {
    for name in ["native_player_context", "native_actor_binding"] {
        let mut missing = player_pack();
        missing.remove(&format!("base/{name}.json"));
        rejected(missing, name);
        let mut unlisted = player_pack();
        edit(&mut unlisted, "manifest.json", |v| {
            v["files"]
                .as_object_mut()
                .unwrap()
                .remove(&format!("base/{name}.json"));
            v["domains"]["base"]
                .as_array_mut()
                .unwrap()
                .retain(|d| d != name);
        });
        rejected(unlisted, "native v2 artifact missing");
        let mut smuggled = pack();
        smuggled.insert(format!("base/{name}.json"), b"{}".to_vec());
        // Even an unlisted file cannot hide a v2 actor policy under v1.
        rejected(smuggled, "cannot appear under native contract v1");
    }
    let mut missing = player_pack();
    edit(&mut missing, NATIVE_RUNTIME_PATH, |v| {
        v.as_object_mut().unwrap().remove("player");
    });
    seal_inventory(&mut missing);
    rejected(missing, "v2 requires player artifacts");
}

fn reseal_player(files: &mut BTreeMap<String, Vec<u8>>) {
    let context_sha = hash(&files["base/native_player_context.json"]);
    edit(files, "base/native_actor_binding.json", |v| {
        v["context_sha256"] = context_sha.clone().into()
    });
    let binding_sha = hash(&files["base/native_actor_binding.json"]);
    edit(files, NATIVE_RUNTIME_PATH, |v| {
        v["player"]["context_sha256"] = context_sha.into();
        v["player"]["actor_binding_sha256"] = binding_sha.into();
    });
    reseal(files);
}

#[test]
fn native_v2_rejects_forged_context_normalization_predicates_and_patches() {
    for (path, pointer, value) in [
        ("base/native_player_context.json", "/realm", json!("CN")),
        (
            "base/native_player_context.json",
            "/game_patch",
            json!("0.5.5.4.2"),
        ),
        (
            "base/native_player_context.json",
            "/facts/character/source/row",
            json!(1),
        ),
        (
            "base/native_player_context.json",
            "/facts/character/values/BaseIntelligence",
            json!(1000),
        ),
        (
            "base/native_actor_binding.json",
            "/critical_bonus/bonus_percent",
            json!(100),
        ),
        (
            "base/native_actor_binding.json",
            "/critical_bonus/meaning/0/origin",
            json!("parent"),
        ),
        (
            "base/native_actor_binding.json",
            "/actor_id",
            json!("Metadata/Characters/Int/Int"),
        ),
        (
            "base/native_actor_binding.json",
            "/excluded_sources",
            json!([]),
        ),
        ("base/native_actor_binding.json", "/dispositions", json!([])),
    ] {
        let mut files = player_pack();
        edit(&mut files, path, |v| {
            *v.pointer_mut(pointer).unwrap() = value
        });
        reseal_player(&mut files);
        assert!(
            pobr_gamedata::NativeData::load(&GameData::from_memory(files)).is_err(),
            "accepted {path}#{pointer}"
        );
    }
    for path in [
        "base/native_player_context.json",
        "base/native_actor_binding.json",
        "base/granted_effect_levels.json",
    ] {
        let mut files = player_pack();
        files.insert(format!("patch/{path}"), b"{}".to_vec());
        rejected(files, "cannot be patched");
    }
    let mut legacy = pack();
    legacy.remove(NATIVE_RUNTIME_PATH);
    edit(&mut legacy, "manifest.json", |v| {
        v["schema_version"] = 3.into();
        v["files"]
            .as_object_mut()
            .unwrap()
            .remove(NATIVE_RUNTIME_PATH);
        v["domains"]["base"]
            .as_array_mut()
            .unwrap()
            .retain(|d| d != "native_runtime");
    });
    assert!(pobr_gamedata::NativeData::load(&GameData::from_memory(legacy)).is_err());
}

#[test]
fn native_v2_ignores_ambient_legacy_defaults_and_does_not_admit_unknown_outputs() {
    let mut files = player_pack();
    let expected = pobr_gamedata::NativeData::load(&GameData::from_memory(files.clone())).unwrap();
    files.insert(
        "overlay-common/character_constants.json".into(),
        b"invalid legacy constants".to_vec(),
    );
    files.insert(
        "overlay-common/skill_overrides.json".into(),
        b"invalid legacy overrides".to_vec(),
    );
    let data = GameData::from_memory(files);
    let native = pobr_gamedata::NativeData::load(&data).unwrap();
    assert_eq!(native.identity(), expected.identity());
    assert_eq!(native.actor_binding(), expected.actor_binding());
    assert!(data.validate_calculation_admission().is_err());
    let mut files = player_pack();
    edit(&mut files, NATIVE_RUNTIME_PATH, |v| {
        v["output_profile"] = "full-dps-v1".into()
    });
    seal_inventory(&mut files);
    rejected(files, "unknown variant");
}
