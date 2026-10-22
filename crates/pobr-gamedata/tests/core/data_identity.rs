//! Data-pack identity agrees across native/staged loaders and all runtime layers.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pobr_gamedata::{DataIdentity, GameData};
use serde_json::json;
use sha2::{Digest, Sha256};

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn files(schema: u32) -> BTreeMap<String, Vec<u8>> {
    let costs = br#"[{"id":"Mana","divisor":1}]"#.to_vec();
    let stats = b"[]".to_vec();
    let manifest = json!({
        "schema_version": schema, "poe_version": "1.2.3", "languages": [],
        "domains": { "base": ["stats", "cost_types"], "overlay": [], "generated": [] },
        "files": { "base/cost_types.json": hash(&costs), "base/stats.json": hash(&stats) }
    });
    BTreeMap::from([
        (
            "manifest.json".into(),
            serde_json::to_vec(&manifest).unwrap(),
        ),
        ("base/cost_types.json".into(), costs),
        ("base/stats.json".into(), stats),
        (
            "overlay-common/special_mods.json".into(),
            br#"{"entries":[]}"#.to_vec(),
        ),
        (
            "patch/base/cost_types.json".into(),
            br#"[{"id":"Mana","divisor":2}]"#.to_vec(),
        ),
    ])
}

struct DiskPack(PathBuf);

impl DiskPack {
    fn new(tag: &str, files: &BTreeMap<String, Vec<u8>>) -> Self {
        let root =
            std::env::temp_dir().join(format!("pobr-data-identity-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("version")).unwrap();
        let pack = Self(root);
        // Reverse sorted order deliberately differs from memory's BTreeMap order.
        for (relative, bytes) in files.iter().rev() {
            let path = pack.path(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        pack
    }

    fn version(&self) -> PathBuf {
        self.0.join("version")
    }

    fn path(&self, relative: &str) -> PathBuf {
        if relative.starts_with("overlay-common/") {
            self.0.join(relative)
        } else {
            self.version().join(relative)
        }
    }
}

impl Drop for DiskPack {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn identity(files: BTreeMap<String, Vec<u8>>) -> DataIdentity {
    GameData::from_memory(files).identity().unwrap().clone()
}

#[test]
fn native_and_memory_identify_the_same_data_layers_and_order() {
    for schema in [1, 2, 3] {
        let files = files(schema);
        let disk = DiskPack::new(&format!("backends-{schema}"), &files);
        let native = GameData::new(disk.version());
        let memory = GameData::from_memory(files);
        assert_eq!(native.identity().unwrap(), memory.identity().unwrap());
        assert_eq!(native.cost_types().unwrap()[0].divisor, 2);
        assert_eq!(memory.cost_types().unwrap()[0].divisor, 2);
        assert_eq!(
            native.identity().unwrap().data_version.as_deref(),
            Some("1.2.3")
        );
        assert_eq!(native.identity().unwrap().sha256.len(), 64);
    }
}

#[test]
fn changes_in_each_effective_layer_change_identity() {
    let original = files(3);
    let initial = identity(original.clone());
    for relative in [
        "base/stats.json",
        "overlay-common/special_mods.json",
        "patch/base/cost_types.json",
    ] {
        let mut changed = original.clone();
        changed.get_mut(relative).unwrap().push(b' ');
        if relative == "base/stats.json" {
            let mut manifest: serde_json::Value =
                serde_json::from_slice(&changed["manifest.json"]).unwrap();
            manifest["files"][relative] = hash(&changed[relative]).into();
            changed.insert(
                "manifest.json".into(),
                serde_json::to_vec(&manifest).unwrap(),
            );
        }
        let disk = DiskPack::new(&format!("changed-{}", relative.replace('/', "-")), &changed);
        let expected = identity(changed);
        assert_ne!(initial, expected, "{relative}");
        assert_eq!(GameData::new(disk.version()).identity().unwrap(), &expected);
    }
    for relative in [
        "overlay-common/special_mods.json",
        "patch/base/cost_types.json",
    ] {
        let mut missing = original.clone();
        missing.remove(relative);
        assert_ne!(initial, identity(missing), "removed {relative}");
    }
}

#[test]
fn metadata_order_and_irrelevant_files_do_not_change_identity() {
    for schema in [1, 2, 3] {
        let mut changed = files(schema);
        let initial = identity(changed.clone());
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&changed["manifest.json"]).unwrap();
        manifest["domains"]["base"] = json!(["cost_types", "stats"]);
        changed.insert(
            "manifest.json".into(),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        );
        for relative in [
            "generated/parsed_mods.json",
            "generated/translation-audit.json",
            "patch/base/missing.json",
            "patch/manifest.json",
            "patch/overlay-common/special_mods.json",
        ] {
            changed.insert(relative.into(), b"not runtime JSON".to_vec());
        }
        if schema == 3 {
            changed.insert("base/unlisted.json".into(), b"not runtime JSON".to_vec());
        }
        assert_eq!(identity(changed.clone()), initial);
        let disk = DiskPack::new(&format!("irrelevant-{schema}"), &changed);
        assert_eq!(GameData::new(disk.version()).identity().unwrap(), &initial);
    }
}

#[test]
fn legacy_identity_includes_unlisted_files_and_missing_manifest() {
    let mut original = files(2);
    original.remove("manifest.json");
    let initial = identity(original.clone());
    assert_eq!(initial.data_version, None);
    let disk = DiskPack::new("no-manifest", &original);
    assert_eq!(GameData::new(disk.version()).identity().unwrap(), &initial);
    original.insert("stats.json".into(), b"[]".to_vec());
    assert_ne!(
        identity(original),
        initial,
        "legacy flat files are part of the input pack"
    );
}

#[test]
fn invalid_sealed_files_and_unsafe_legacy_keys_have_no_identity() {
    for missing in [false, true] {
        let mut broken = files(3);
        if missing {
            broken.remove("base/stats.json");
        } else {
            broken.insert("base/stats.json".into(), b"[0]".to_vec());
        }
        let disk = DiskPack::new(&format!("invalid-{missing}"), &broken);
        assert!(GameData::new(disk.version()).identity().is_err());
        assert!(GameData::from_memory(broken).identity().is_err());
    }
    for key in [
        "./base/stats.json",
        "../outside.json",
        "base\\stats.json",
        "/absolute.json",
        "overlay-common/../outside.json",
    ] {
        let mut broken = files(2);
        broken.insert(key.into(), b"[]".to_vec());
        assert!(GameData::from_memory(broken).identity().is_err(), "{key}");
    }
}

#[test]
fn identity_and_errors_are_cached_for_the_immutable_loader_lifetime() {
    let original = files(3);
    let disk = DiskPack::new("cached", &original);
    let data = GameData::new(disk.version());
    let cloned = data.clone();
    let initial = data.identity().unwrap().clone();
    std::fs::write(disk.path("patch/base/cost_types.json"), b"[]").unwrap();
    assert_eq!(data.identity().unwrap(), &initial);
    assert!(std::ptr::eq(
        data.identity().unwrap(),
        cloned.identity().unwrap()
    ));
    assert_ne!(GameData::new(disk.version()).identity().unwrap(), &initial);

    std::fs::remove_file(disk.path("base/stats.json")).unwrap();
    let invalid = GameData::new(disk.version());
    assert!(invalid.identity().is_err());
    std::fs::write(disk.path("base/stats.json"), b"[]").unwrap();
    assert!(invalid.identity().is_err(), "repairs require a new loader");
    assert!(GameData::new(disk.version()).identity().is_ok());
}

#[cfg(unix)]
#[test]
fn discovery_rejects_directory_symlink_cycles() {
    let disk = DiskPack::new("symlink", &files(3));
    let common = disk.0.join("overlay-common");
    std::os::unix::fs::symlink(Path::new("."), common.join("loop")).unwrap();
    assert!(GameData::new(disk.version()).identity().is_err());
}
