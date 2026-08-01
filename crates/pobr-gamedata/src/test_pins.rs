//! Test-pin snapshots: blessed snapshots for data-content-count pins (v0.0.3).
//!
//! Test pins of the form "asserted value = a count/statistic of the
//! current data file's content" (section entry counts, coverage, a
//! specific data value) necessarily change on every data regen, so
//! manually re-pinning them is zero-information labor. This class of pin
//! is stored uniformly in `data/<ver>/generated/test_pins.json` (a flat
//! map: pin name → JSON value), checked by tests via [`assert_pin`];
//! after a regen, rerunning the same tests with `POBR_BLESS_PINS=1`
//! refreshes them in one step (the last step of `pipeline/regen-all.sh`
//! already orchestrates this).
//!
//! Boundary: **structural guards** (a non-empty schema, monotonic
//! ratchets, dedup invariants) still live hardcoded in the code, not in
//! the snapshot — the snapshot only carries values that drift with data
//! content, where the drift itself has no right or wrong answer.
//!
//! Note: bless write-back is serialized within a process (a global lock);
//! concurrent bless across processes on the same file will clobber each
//! other over the whole file. Use `cargo test` (single process, multiple
//! threads) rather than nextest (per-process/per-test) to run bless commands.

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
/// from (a golden test passes the directory for
/// `GOLDEN_PARITY_DATA_VERSION`, an active-version test passes the
/// directory for `DATA_VERSION`) — a pin lives alongside its data, so
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
                 with POBR_BLESS_PINS=1 (the last step of pipeline/regen-all.sh already batches \
                 the refresh and commits the snapshot).",
                path.display()
            );
        }
    }
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
