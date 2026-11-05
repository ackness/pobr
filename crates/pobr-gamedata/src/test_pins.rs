//! Reviewed test-pin snapshots for data-content counts and statistics.
//!
//! Repository count/statistic baselines live in a small reviewed fixture,
//! bound to the game part of the prepared manifest ([`game_manifest_sha256`]:
//! a language-only release needs no rebind), and are checked through
//! [`assert_repository_pin`]. They are test metadata, never runtime payloads.
//! Missing versions, manifest changes and value drift require explicit review.
//!
//! [`assert_pin`] retains the legacy generated-file API for explicit local or
//! temporary snapshots; only that API supports opt-in `POBR_BLESS_PINS=1` writes.
//! Structural guards remain in the tests independently of either snapshot API.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use serde_json::Value;

/// The bless-toggle environment variable: when `POBR_BLESS_PINS=1`,
/// [`assert_pin`] writes the actual value back to the snapshot.
pub const BLESS_ENV: &str = "POBR_BLESS_PINS";

/// The bless write-back lock (serializes read-modify-write on the same
/// snapshot file across test threads within a process).
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Checks (or, in bless mode, writes back) one pin in
/// `<version_dir>/generated/test_pins.json`.
///
/// `version_dir` = the version directory the test actually loaded its data
/// from, normally `current_data_dir()` — a pin lives alongside its data, so
/// regenning one version only refreshes that version's snapshot.
///
/// Normal mode: a missing pin or a value mismatch → panics, with the error
/// message giving the bless-refresh command.
/// Bless mode (`POBR_BLESS_PINS=1`): writes back the actual value and passes.
pub fn assert_pin(version_dir: &Path, name: &str, actual: impl Into<Value>) {
    let actual = actual.into();
    let path = version_dir.join("generated/test_pins.json");

    if std::env::var(BLESS_ENV).is_ok_and(|v| v == "1") {
        let _guard = WRITE_LOCK.lock().unwrap();
        let mut pins = read_pins(&path);
        pins.insert(name.to_string(), actual);
        let mut json = serde_json::to_string_pretty(&pins).expect("serialize test pins");
        json.push('\n');
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap_or_else(|e| {
                panic!("create {} failed: {e}", parent.display());
            });
        }
        std::fs::write(&path, json)
            .unwrap_or_else(|e| panic!("write {} failed: {e}", path.display()));
        return;
    }

    let pins = read_pins(&path);
    match pins.get(name) {
        Some(expected) if *expected == actual => {}
        found => {
            let expected = found.map_or("<missing>".to_string(), Value::to_string);
            panic!(
                "test pin `{name}` out of date in {}:\n  blessed: {expected}\n  actual:  {actual}\n\
                 The data content changed (expected after a regen). To refresh: rerun this test \
                 with POBR_BLESS_PINS=1 after reviewing the change; regeneration never blesses \
                 golden snapshots automatically.",
                path.display()
            );
        }
    }
}

/// Checks reviewed repository regression metadata against an exact prepared pack.
/// Update the fixture and manifest binding explicitly after reviewing a data
/// change; this never writes into immutable imported runtime data.
pub fn assert_repository_pin(version_dir: &Path, name: &str, actual: impl Into<Value>) {
    let baselines: Value =
        serde_json::from_str(include_str!("../tests/fixtures/repository-test-pins.json"))
            .expect("parse reviewed repository test pins");
    assert_repository_pin_in(version_dir, name, actual.into(), &baselines);
}

/// The digest the repository pins bind to: the prepared manifest without
/// what the language package contributes (`languages` and the `i18n/`
/// entries of `files`), as compact JSON with sorted keys. The pins count
/// game data only, so a language-only release keeps this digest.
pub fn game_manifest_sha256(manifest: &Value) -> String {
    let mut game = manifest.clone();
    if let Some(object) = game.as_object_mut() {
        object.remove("languages");
        if let Some(files) = object.get_mut("files").and_then(Value::as_object_mut) {
            files.retain(|path, _| !path.starts_with("i18n/"));
        }
    }
    let bytes = serde_json::to_vec(&game).expect("serialize the game manifest");
    crate::manifest::sha256_hex(&bytes)
}

fn assert_repository_pin_in(version_dir: &Path, name: &str, actual: Value, baselines: &Value) {
    let manifest = std::fs::read(version_dir.join("manifest.json"))
        .expect("prepare the exact external data pack before testing");
    let identity: Value = serde_json::from_slice(&manifest).expect("parse prepared manifest");
    let version = identity["poe_version"].as_str().expect("manifest version");
    let baseline = &baselines[version];
    assert_eq!(
        Some(game_manifest_sha256(&identity).as_str()),
        baseline["game_manifest_sha256"].as_str(),
        "reviewed test-pin manifest binding for {version} differs"
    );
    let expected = baseline["pins"]
        .as_object()
        .and_then(|pins| pins.get(name))
        .expect("missing reviewed repository test pin");
    assert_eq!(
        expected, &actual,
        "reviewed repository test pin {version}/{name} differs; review the data change and fixture"
    );
}

/// Reads the snapshot (a missing file → an empty map; a corrupt file →
/// panics, not silenced).
fn read_pins(path: &Path) -> BTreeMap<String, Value> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .unwrap_or_else(|e| panic!("parse {} failed: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
        Err(e) => panic!("read {} failed: {e}", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_version_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pobr-test-pins-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn repository_fixture(tag: &str) -> (std::path::PathBuf, Value) {
        let dir = temp_version_dir(tag);
        let manifest = br#"{"files":{"base/a.json":"aa","i18n/zh-CN/a.json":"cc"},"languages":["zh-CN"],"poe_version":"fixture"}"#;
        std::fs::write(dir.join("manifest.json"), manifest).unwrap();
        let game = br#"{"files":{"base/a.json":"aa"},"poe_version":"fixture"}"#;
        let baseline = serde_json::json!({"fixture": {
            "game_manifest_sha256": crate::manifest::sha256_hex(game),
            "pins": {"a.count": 3}
        }});
        (dir, baseline)
    }

    #[test]
    fn repository_pin_matches_exact_manifest_and_value() {
        let (dir, baseline) = repository_fixture("repository-match");
        assert_repository_pin_in(&dir, "a.count", 3.into(), &baseline);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[should_panic(expected = "manifest binding")]
    fn repository_pin_rejects_changed_game_manifest() {
        let (dir, baseline) = repository_fixture("repository-manifest");
        std::fs::write(
            dir.join("manifest.json"),
            br#"{"files":{"base/a.json":"ab","i18n/zh-CN/a.json":"cc"},"languages":["zh-CN"],"poe_version":"fixture"}"#,
        )
        .unwrap();
        assert_repository_pin_in(&dir, "a.count", 3.into(), &baseline);
    }

    #[test]
    fn repository_pin_ignores_a_language_only_change() {
        let (dir, baseline) = repository_fixture("repository-language");
        std::fs::write(
            dir.join("manifest.json"),
            br#"{ "files": {"base/a.json":"aa", "i18n/zh-CN/a.json":"cd", "i18n/zh-TW/a.json":"dd"},
                 "languages": ["zh-CN", "zh-TW"], "poe_version": "fixture" }"#,
        )
        .unwrap();
        assert_repository_pin_in(&dir, "a.count", 3.into(), &baseline);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[should_panic(expected = "repository test pin")]
    fn repository_pin_rejects_changed_value() {
        let (dir, baseline) = repository_fixture("repository-value");
        assert_repository_pin_in(&dir, "a.count", 4.into(), &baseline);
    }

    #[test]
    #[should_panic(expected = "missing reviewed repository test pin")]
    fn repository_pin_never_treats_absent_as_null() {
        let (dir, baseline) = repository_fixture("repository-missing");
        assert_repository_pin_in(&dir, "missing", Value::Null, &baseline);
    }

    #[test]
    fn bless_then_match_roundtrip() {
        let dir = temp_version_dir("roundtrip");
        // ponytail: env var is process-global — this test only exercises the
        // non-bless read path directly via a hand-written snapshot file.
        std::fs::create_dir_all(dir.join("generated")).unwrap();
        std::fs::write(
            dir.join("generated/test_pins.json"),
            r#"{"a.count": 3, "b.obj": {"x": 2.5}}"#,
        )
        .unwrap();
        assert_pin(&dir, "a.count", 3);
        assert_pin(&dir, "b.obj", serde_json::json!({"x": 2.5}));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    #[should_panic(expected = "out of date")]
    fn mismatch_panics_with_bless_hint() {
        let dir = temp_version_dir("mismatch");
        std::fs::create_dir_all(dir.join("generated")).unwrap();
        std::fs::write(dir.join("generated/test_pins.json"), r#"{"a.count": 3}"#).unwrap();
        assert_pin(&dir, "a.count", 4);
    }

    #[test]
    #[should_panic(expected = "out of date")]
    fn missing_pin_panics() {
        let dir = temp_version_dir("missing");
        assert_pin(&dir, "no.such.pin", 1);
    }
}
